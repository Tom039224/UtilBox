import { useEffect, useState } from "react";
import { Film, FolderOpen, Minimize2, Play, RefreshCw, X } from "lucide-react";
import {
  api,
  errorMessage,
  listenJob,
  newJobId,
  type AppInfo,
  type CompressResponse,
  type JobProgress,
  type MediaInfo,
  type ToolInfo,
} from "../lib/api";
import {
  baseName,
  dirName,
  extOf,
  formatBytes,
  formatDuration,
  formatMs,
  joinPath,
  reductionPercent,
  stripExt,
  VIDEO_FILTERS,
} from "../lib/format";
import { Dropzone } from "../components/Dropzone";
import { isAndroid, openOutput, pickSavePath, revealOutput } from "../lib/platform";
import { InlineAlert, ProgressPanel } from "../components/ProgressPanel";
import { PageHeader } from "../components/PageHeader";
import {
  AUDIO_OPTIONS,
  CODEC_LABELS,
  CODEC_NOTES,
  CRF_RANGES,
  DEFAULT_CRF,
  QUALITY_PRESETS,
  RESOLUTIONS,
  type Codec,
} from "../lib/compression";

export function CompressPage({
  active,
  info,
  tools,
  gpuAv1,
}: {
  active: boolean;
  info: AppInfo | null;
  tools: ToolInfo[];
  gpuAv1: boolean;
}) {
  const [input, setInput] = useState("");
  const [media, setMedia] = useState<MediaInfo | null>(null);
  const [probing, setProbing] = useState(false);
  const [output, setOutput] = useState("");
  const [outputTouched, setOutputTouched] = useState(false);
  const [mode, setMode] = useState<"crf" | "size">("crf");
  const [crf, setCrf] = useState(24);
  const [targetMb, setTargetMb] = useState(100);
  const [height, setHeight] = useState(0);
  const [codec, setCodec] = useState<Codec>("h264");
  const [audioKbps, setAudioKbps] = useState(128);
  const [removeAudio, setRemoveAudio] = useState(false);

  const [running, setRunning] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [result, setResult] = useState<CompressResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const available = new Set(
    tools.filter((tool) => tool.available).map((tool) => tool.name),
  );
  const ready = available.has("ffmpeg") && available.has("ffprobe");

  useEffect(() => {
    if (input && !outputTouched) {
      const dir = info?.defaultOutputDir || dirName(input);
      const suffix = isAndroid ? "-compressed" : "_compressed";
      setOutput(joinPath(dir, `${stripExt(baseName(input))}${suffix}.mp4`));
    }
  }, [input, info, outputTouched]);

  useEffect(() => {
    if (!active) return;
    setError((prev) => (prev?.includes("キャンセル") ? null : prev));
  }, [active]);

  const selectInput = async (paths: string[]) => {
    const path = paths[0];
    if (!path) return;
    const ext = path.split(".").pop()?.toLowerCase() ?? "";
    if (!VIDEO_FILTERS.extensions.includes(ext)) {
      setError("動画ファイル（MP4 / MKV / MOV / WebM / AVI など）を選択してください。");
      return;
    }
    setInput(path);
    setOutputTouched(false);
    setMedia(null);
    setResult(null);
    setError(null);
    setProbing(true);
    try {
      const infoResult = await api.probeMedia(path);
      if (!infoResult.width || !infoResult.height) {
        setError("このファイルには映像ストリームがありません。");
        setInput("");
        return;
      }
      setMedia(infoResult);
      setTargetMb(
        Math.max(5, Math.round((infoResult.size / (1024 * 1024)) * 0.6)),
      );
      if (height > 0 && infoResult.height && height > infoResult.height) {
        setHeight(0);
      }
    } catch (e) {
      setError(errorMessage(e));
      setInput("");
    } finally {
      setProbing(false);
    }
  };

  const chooseOutput = async () => {
    try {
      const chosen = await pickSavePath(output, "圧縮後のファイル名", [
        { name: "MP4 動画", extensions: ["mp4"] },
      ]);
      if (chosen) {
        const normalized = chosen.toLowerCase().endsWith(".mp4")
          ? chosen
          : `${chosen}.mp4`;
        setOutput(normalized);
        setOutputTouched(true);
      }
    } catch {
      /* cancelled */
    }
  };

  const start = async () => {
    if (running || !input || !output) return;
    const id = newJobId("compress");
    setJobId(id);
    setRunning(true);
    setProgress(null);
    setResult(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listenJob(id, { onProgress: setProgress });
      const res = await api.compressVideo(id, {
        input,
        output,
        mode,
        crf: mode === "crf" ? crf : null,
        targetMb: mode === "size" ? targetMb : null,
        maxHeight: height > 0 ? height : null,
        codec,
        audioKbps: removeAudio ? null : audioKbps,
        removeAudio,
        preset: "medium",
        audioMode: "encode",
      });
      setResult(res);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      unlisten?.();
      setRunning(false);
      setJobId(null);
    }
  };

  const cancel = async () => {
    if (!jobId) return;
    try {
      await api.cancelJob(jobId);
      setProgress((prev) =>
        prev ? { ...prev, phase: "キャンセル", message: "エンコードを停止しています…" } : prev,
      );
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const reset = () => {
    setInput("");
    setMedia(null);
    setOutput("");
    setOutputTouched(false);
    setResult(null);
    setError(null);
    setProgress(null);
  };

  const sourceHeight = media?.height ?? 0;
  const activePreset = QUALITY_PRESETS[codec].find((preset) => preset.crf === crf);

  const changeCodec = (next: Codec) => {
    setCodec(next);
    setCrf(DEFAULT_CRF[next]);
  };

  return (
    <section className="sheet" hidden={!active}>
      <div className="compress">
        <div className="compress__main">
          <PageHeader
            title="MP4 圧縮"
            description="H.264 / H.265 / AV1 で画質と容量のバランスを調整します。目標サイズを指定する 2 パス圧縮にも対応しています。"
            meta={<span>FFmpeg {ready ? "利用可能" : "未検出"}</span>}
          />

          <div className="stack alerts">
            {!ready && (
              <InlineAlert variant="warn">
                FFmpeg / FFprobe が見つかりません。「ツール設定」から診断してください。
              </InlineAlert>
            )}
            {error && (
              <InlineAlert variant="error" onClose={() => setError(null)}>
                {error}
              </InlineAlert>
            )}
          </div>

          {!input ? (
            <Dropzone
              active={active}
              multiple={false}
              filters={[VIDEO_FILTERS]}
              kind="video"
              onFiles={selectInput}
              title={isAndroid ? "圧縮する動画を選択" : "圧縮する動画をドロップ"}
              subtitle="MP4 / MKV / MOV / WebM / AVI / M4V / TS / FLV / WMV"
              disabled={running}
            />
          ) : (
            <section className="panel" aria-labelledby="compress-source">
              <div className="panel__head">
                <h2 className="panel__title" id="compress-source">
                  素材
                </h2>
                <button
                  type="button"
                  className="icon-btn icon-btn--sm"
                  onClick={reset}
                  disabled={running}
                  aria-label="選択を解除"
                >
                  <X size={14} />
                </button>
              </div>
              <div className="panel__body panel__body--tight">
                <div className="compress-file">
                  <Film size={16} aria-hidden="true" />
                  <span className="compress-file__name" title={input}>
                    {baseName(input)}
                  </span>
                  <span className="tag">.{extOf(input) || "?"}</span>
                </div>

                {probing ? (
                  <p className="compress-probing">
                    <RefreshCw size={13} className="spin" aria-hidden="true" />
                    解析中…
                  </p>
                ) : media ? (
                  <table className="table compress-readout">
                    <tbody>
                      <tr>
                        <th scope="row">長さ</th>
                        <td className="num">{formatDuration(media.durationSec)}</td>
                      </tr>
                      <tr>
                        <th scope="row">解像度</th>
                        <td className="num">
                          {media.width && media.height
                            ? `${media.width}×${media.height}`
                            : "—"}
                        </td>
                      </tr>
                      <tr>
                        <th scope="row">映像コーデック</th>
                        <td className="num">{media.videoCodec ?? "—"}</td>
                      </tr>
                      <tr>
                        <th scope="row">音声コーデック</th>
                        <td className="num">{media.audioCodec ?? "—"}</td>
                      </tr>
                      <tr>
                        <th scope="row">ビットレート</th>
                        <td className="num">
                          {media.bitrateKbps ? `${media.bitrateKbps} kbps` : "—"}
                        </td>
                      </tr>
                      <tr>
                        <th scope="row">フレームレート</th>
                        <td className="num">
                          {media.fps ? `${media.fps.toFixed(0)} fps` : "—"}
                        </td>
                      </tr>
                      <tr>
                        <th scope="row">コンテナ</th>
                        <td className="num">{media.container ?? "—"}</td>
                      </tr>
                      <tr>
                        <th scope="row">サイズ</th>
                        <td className="num">{formatBytes(media.size)}</td>
                      </tr>
                    </tbody>
                  </table>
                ) : (
                  <p className="field__note">解析できませんでした</p>
                )}
              </div>
            </section>
          )}

          {result && <CompressResult result={result} onReset={reset} />}

          <aside className="note">
            <span className="note__title">おすすめ</span>
            <span>
              まずは「標準（CRF 24）」で試してください。SNS 投稿なら 720p / CRF 26
              前後、保存用なら CRF 20 前後が目安です。AV1 はさらに小さくなりますが、
              エンコードに時間がかかり、古いプレイヤーでは再生できない場合があります。
            </span>
            <span>動画は外部に送信されません。エンコードは端末内の FFmpeg で実行されます。</span>
          </aside>
        </div>

        <aside className="compress__side">
          <section className="panel" aria-labelledby="compress-settings">
            <div className="panel__head">
              <h2 className="panel__title" id="compress-settings">
                圧縮設定
              </h2>
            </div>

            <div className="panel__body panel__body--split">
              <div className="cell">
                <span className="field__label" id="compress-mode">
                  指定方法
                </span>
                <div className="seg" role="group" aria-labelledby="compress-mode">
                  <button
                    type="button"
                    className="seg__btn"
                    aria-pressed={mode === "crf"}
                    onClick={() => setMode("crf")}
                    disabled={running}
                  >
                    画質
                  </button>
                  <button
                    type="button"
                    className="seg__btn"
                    aria-pressed={mode === "size"}
                    onClick={() => setMode("size")}
                    disabled={running}
                  >
                    サイズ
                  </button>
                </div>
              </div>

              <div className={gpuAv1 ? "cell cell--wide" : "cell"}>
                <span className="field__label" id="compress-codec">
                  コーデック
                </span>
                <div className="seg" role="group" aria-labelledby="compress-codec">
                  {(["h264", "h265", "av1", ...(gpuAv1 ? ["av1_gpu"] : [])] as Codec[]).map((option) => (
                    <button
                      key={option}
                      type="button"
                      className="seg__btn"
                      aria-pressed={codec === option}
                      onClick={() => changeCodec(option)}
                      disabled={running}
                    >
                      {CODEC_LABELS[option]}
                    </button>
                  ))}
                </div>
                <p className="field__note">{CODEC_NOTES[codec]}</p>
              </div>

              <div className="cell cell--wide">
                {mode === "crf" ? (
                  <>
                    <div className="row row--between">
                      <label className="field__label" htmlFor="crf-range">
                        画質（CRF）
                      </label>
                      <span className="count num">CRF {crf}</span>
                    </div>
                    <input
                      id="crf-range"
                      className="compress-range"
                      type="range"
                      min={CRF_RANGES[codec].min}
                      max={CRF_RANGES[codec].max}
                      step={1}
                      value={crf}
                      onChange={(event) => setCrf(Number(event.target.value))}
                      disabled={running}
                    />
                    <div
                      className="seg compress-seg"
                      role="group"
                      aria-label="画質のプリセット"
                    >
                      {QUALITY_PRESETS[codec].map((preset) => (
                        <button
                          key={preset.crf}
                          type="button"
                          className="seg__btn"
                          aria-pressed={crf === preset.crf}
                          onClick={() => setCrf(preset.crf)}
                          title={preset.hint}
                          disabled={running}
                        >
                          {preset.label}
                        </button>
                      ))}
                    </div>
                    <p className="field__note">
                      {activePreset
                        ? activePreset.hint
                        : "スライダーで任意の値を指定できます。"}
                    </p>
                  </>
                ) : (
                  <>
                    <div className="row row--between">
                      <label className="field__label" htmlFor="target-size">
                        目標サイズ（MB）
                      </label>
                      <span className="count num">{targetMb} MB</span>
                    </div>
                    <input
                      id="target-size"
                      className="input input--mono"
                      type="number"
                      min={1}
                      max={100000}
                      value={targetMb}
                      onChange={(event) =>
                        setTargetMb(Math.max(1, Number(event.target.value) || 1))
                      }
                      disabled={running}
                    />
                    <input
                      className="compress-range"
                      type="range"
                      min={5}
                      max={Math.max(300, Math.round((media?.size ?? 0) / (1024 * 1024)))}
                      step={5}
                      value={targetMb}
                      onChange={(event) => setTargetMb(Number(event.target.value))}
                      disabled={running}
                      aria-label="目標サイズのスライダー"
                    />
                    <p className="field__note">
                      2 パスで目標ビットレートを計算します。元ファイル{" "}
                      {formatBytes(media?.size ?? 0)} の約{" "}
                      {media?.size
                        ? Math.round(((targetMb * 1024 * 1024) / media.size) * 100)
                        : 0}
                      % を目指します。
                    </p>
                  </>
                )}
              </div>

              <div className="cell">
                <label className="field__label" htmlFor="compress-height">
                  解像度
                </label>
                <select
                  id="compress-height"
                  className="input"
                  value={height}
                  onChange={(event) => setHeight(Number(event.target.value))}
                  disabled={running}
                >
                  {RESOLUTIONS.map((res) => {
                    const unavailable =
                      res.value > 0 && sourceHeight > 0 && res.value > sourceHeight;
                    return (
                      <option key={res.value} value={res.value} disabled={unavailable}>
                        {unavailable ? `${res.label}（元動画より大）` : res.label}
                      </option>
                    );
                  })}
                </select>
              </div>

              <div className="cell">
                <label className="field__label" htmlFor="compress-audio">
                  音声
                </label>
                <select
                  id="compress-audio"
                  className="input"
                  value={removeAudio ? "none" : String(audioKbps)}
                  onChange={(event) => {
                    const next = event.target.value;
                    if (next === "none") {
                      setRemoveAudio(true);
                    } else {
                      setRemoveAudio(false);
                      setAudioKbps(Number(next));
                    }
                  }}
                  disabled={running}
                >
                  {AUDIO_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>

              {isAndroid ? (
                <div className="cell cell--wide">
                  <span className="field__label">保存先</span>
                  <p className="field__note">Download/UtilBox</p>
                </div>
              ) : (
              <div className="cell cell--wide">
                <span className="field__label" id="compress-output">
                  保存先
                </span>
                <button
                  type="button"
                  className="pick"
                  onClick={chooseOutput}
                  disabled={running || !input}
                  title={output}
                  aria-labelledby="compress-output"
                >
                  <FolderOpen size={15} aria-hidden="true" />
                  <span className="pick__text">
                    <span className="pick__value">
                      {output || "動画を選択してください"}
                    </span>
                  </span>
                </button>
              </div>
              )}

            </div>

            <div className="panel__foot">
              <ProgressPanel
                embedded
                progress={progress}
                running={running}
                onCancel={cancel}
                label="エンコード中"
              />
              <button
                type="button"
                className="btn btn--primary btn--block"
                onClick={start}
                disabled={running || !input || !output || !ready}
              >
                {running ? (
                  <>
                    <RefreshCw size={16} className="spin" aria-hidden="true" />
                    圧縮中…
                  </>
                ) : (
                  <>
                    <Minimize2 size={16} aria-hidden="true" />
                    圧縮を開始
                  </>
                )}
              </button>
            </div>
          </section>
        </aside>
      </div>
    </section>
  );
}

function CompressResult({
  result,
  onReset,
}: {
  result: CompressResponse;
  onReset: () => void;
}) {
  const saved = result.sizeBefore - result.sizeAfter;
  const reduction = reductionPercent(result.sizeBefore, result.sizeAfter);
  const bigger = reduction < 0;
  return (
    <section className="panel" aria-labelledby="compress-result">
      <div className="panel__head">
        <h2 className="panel__title" id="compress-result">
          圧縮結果
        </h2>
        <span className="panel__meta mono">
          {bigger ? `+${Math.abs(reduction)}%` : `-${reduction}%`}
        </span>
      </div>
      <div className="panel__body panel__body--tight">
        {isAndroid && <p className="field__note save-note">保存先: Download/UtilBox</p>}
        <table className="table compress-readout">
          <tbody>
            <tr>
              <th scope="row">元のサイズ</th>
              <td className="num">{formatBytes(result.sizeBefore)}</td>
            </tr>
            <tr>
              <th scope="row">圧縮後</th>
              <td className="num">{formatBytes(result.sizeAfter)}</td>
            </tr>
            <tr>
              <th scope="row">削減</th>
              <td className="num">
                {saved >= 0 ? formatBytes(saved) : `+${formatBytes(-saved)}`}
              </td>
            </tr>
            <tr>
              <th scope="row">削減率</th>
              <td className="num">
                {bigger ? `+${Math.abs(reduction)}%` : `-${reduction}%`}
              </td>
            </tr>
            <tr>
              <th scope="row">所要時間</th>
              <td className="num">{formatMs(result.elapsedMs)}</td>
            </tr>
            <tr>
              <th scope="row">エンコーダー</th>
              <td className="num">{result.encoder}</td>
            </tr>
            {result.media && (
              <tr>
                <th scope="row">解像度</th>
                <td className="num">
                  {result.media.width}×{result.media.height}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="panel__foot">
        <div className="compress-actions">
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={() => openOutput(result.output).catch(() => undefined)}
          >
            <Play size={14} aria-hidden="true" />
            {isAndroid ? "開く" : "再生"}
          </button>
          {!isAndroid && (
            <button
              type="button"
              className="btn btn--quiet btn--sm"
              onClick={() => revealOutput(result.output).catch(() => undefined)}
            >
              <FolderOpen size={14} aria-hidden="true" />
              フォルダを開く
            </button>
          )}
          <button
            type="button"
            className="btn btn--quiet btn--sm compress-actions__reset"
            onClick={onReset}
          >
            別の動画を圧縮
          </button>
        </div>
      </div>
    </section>
  );
}
