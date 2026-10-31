import { useEffect, useRef, useState } from "react";
import {
  Download,
  Film,
  FolderOpen,
  Loader2,
  RefreshCw,
  Search,
} from "lucide-react";
import {
  api,
  errorMessage,
  listenJob,
  newJobId,
  type AppInfo,
  type DownloadResponse,
  type JobProgress,
  type ToolInfo,
  type YtInfo,
} from "../lib/api";
import {
  formatBytes,
  formatCount,
  formatDuration,
  formatMs,
  reductionPercent,
} from "../lib/format";
import { openOutput, pickFolder, revealOutput } from "../lib/platform";
import { InlineAlert, ProgressPanel } from "../components/ProgressPanel";
import { PageHeader } from "../components/PageHeader";
import {
  CODEC_LABELS,
  CODEC_NOTES,
  DEFAULT_CRF,
  QUALITY_PRESETS,
  RESOLUTIONS,
  type Codec,
} from "../lib/compression";

const QUALITIES = [
  { value: "best", label: "最高画質" },
  { value: "2160", label: "2160p (4K)" },
  { value: "1440", label: "1440p" },
  { value: "1080", label: "1080p" },
  { value: "720", label: "720p" },
  { value: "480", label: "480p" },
  { value: "360", label: "360p" },
];

const AUDIO_FORMATS = [
  { value: "mp3", label: "MP3", hint: "どこでも再生" },
  { value: "m4a", label: "M4A", hint: "AAC / 高音質" },
  { value: "opus", label: "Opus", hint: "高効率" },
];

const COOKIES = [
  { value: "", label: "使用しない" },
  { value: "chrome", label: "Chrome" },
  { value: "chromium", label: "Chromium" },
  { value: "firefox", label: "Firefox" },
  { value: "edge", label: "Edge" },
  { value: "brave", label: "Brave" },
  { value: "safari", label: "Safari" },
  { value: "opera", label: "Opera" },
  { value: "vivaldi", label: "Vivaldi" },
];

export function DownloadPage({
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
  const [url, setUrl] = useState("");
  const [yt, setYt] = useState<YtInfo | null>(null);
  const [probing, setProbing] = useState(false);
  const [mode, setMode] = useState<"video" | "audio">("video");
  const [quality, setQuality] = useState("1080");
  const [audioFormat, setAudioFormat] = useState<"mp3" | "m4a" | "opus">("mp3");
  const [embedThumbnail, setEmbedThumbnail] = useState(true);
  const [embedMetadata, setEmbedMetadata] = useState(true);
  const [cookies, setCookies] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [compress, setCompress] = useState(false);
  const [compressCodec, setCompressCodec] = useState<Codec>("h264");
  const [compressCrf, setCompressCrf] = useState(DEFAULT_CRF.h264);
  const [compressHeight, setCompressHeight] = useState(0);

  const [running, setRunning] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [result, setResult] = useState<DownloadResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const ytDlpReady = tools.some((tool) => tool.name === "yt-dlp" && tool.available);
  const ffmpegReady = tools.some((tool) => tool.name === "ffmpeg" && tool.available);

  // Follow the default folder from settings until the user picks one here.
  const knownDefaultDir = useRef<string | null>(null);
  useEffect(() => {
    const next = info?.defaultOutputDir;
    if (!next) return;
    const previous = knownDefaultDir.current;
    knownDefaultDir.current = next;
    setOutputDir((current) => (!current || current === previous ? next : current));
  }, [info?.defaultOutputDir]);

  const probe = async () => {
    const trimmed = url.trim();
    if (!trimmed || probing || running) return;
    setProbing(true);
    setError(null);
    setYt(null);
    setResult(null);
    try {
      const meta = await api.ytProbe(trimmed);
      setYt(meta);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setProbing(false);
    }
  };

  const chooseOutputDir = async () => {
    try {
      const dir = await pickFolder("保存先フォルダ");
      if (dir) setOutputDir(dir);
    } catch {
      /* cancelled */
    }
  };

  const start = async () => {
    console.log("DBG start", { yt: !!yt, running, outputDir, ytDlpReady, ffmpegReady });
    if (!yt || running || !outputDir) return;
    const id = newJobId("download");
    setJobId(id);
    setRunning(true);
    setProgress(null);
    setResult(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listenJob(id, { onProgress: setProgress });
      const res = await api.downloadMedia(id, {
        url: yt.webpageUrl,
        outputDir,
        mode,
        quality: mode === "video" ? quality : "best",
        audioFormat: mode === "audio" ? audioFormat : null,
        embedThumbnail,
        embedMetadata,
        cookiesBrowser: cookies || null,
        compression:
          mode === "video" && compress
            ? {
                mode: "crf",
                crf: compressCrf,
                targetMb: null,
                maxHeight: compressHeight > 0 ? compressHeight : null,
                codec: compressCodec,
                audioKbps: 128,
                removeAudio: false,
                preset: "medium",
                audioMode: "encode",
              }
            : null,
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
        prev ? { ...prev, phase: "キャンセル", message: "ダウンロードを停止しています…" } : prev,
      );
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const compression = result?.compression ?? null;
  const reduction = compression
    ? reductionPercent(compression.sizeBefore, compression.sizeAfter)
    : 0;
  const saved = compression ? compression.sizeBefore - compression.sizeAfter : 0;
  const audioHint = AUDIO_FORMATS.find((item) => item.value === audioFormat)?.hint;

  return (
    <section className="sheet" hidden={!active}>
      <div className="download">
        <div className="download__main">
          <PageHeader
            title="YouTube ダウンロード"
            description="yt-dlp と FFmpeg で動画・音声を保存。取得時だけネットワークを使い、処理はすべて端末内で行います。"
          />

          <div className="stack alerts">
            {!ytDlpReady && (
              <InlineAlert variant="warn">
                yt-dlp が見つかりません。同梱バイナリを配置するか、「ツール設定」から更新してください。
              </InlineAlert>
            )}
            {!ffmpegReady && (
              <InlineAlert variant="warn">
                FFmpeg が見つかりません。動画の結合や音声変換に必要です。
              </InlineAlert>
            )}
            {error && (
              <InlineAlert variant="error" onClose={() => setError(null)}>
                {error}
              </InlineAlert>
            )}
          </div>

          <div className="field">
            <label className="field__label" htmlFor="download-url">
              URL
            </label>
            <div className="download-bar">
              <input
                id="download-url"
                className="input"
                type="url"
                value={url}
                placeholder="https://www.youtube.com/watch?v=…"
                onChange={(event) => {
                  setUrl(event.target.value);
                  setYt(null);
                  setResult(null);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") probe();
                }}
                disabled={running}
                spellCheck={false}
              />
              <button
                type="button"
                className="btn"
                onClick={probe}
                disabled={!url.trim() || probing || running}
              >
                {probing ? (
                  <Loader2 size={15} className="spin" aria-hidden="true" />
                ) : (
                  <Search size={15} aria-hidden="true" />
                )}
                {probing ? "解析中" : "解析"}
              </button>
            </div>
            <p className="field__note">
              YouTube のほか、yt-dlp が対応するサイトの URL を利用できます。再生リストは先頭の動画を取得します。
            </p>
          </div>

          {yt && (
            <section className="panel" aria-labelledby="download-source">
              <div className="panel__head">
                <h2 className="panel__title" id="download-source">
                  取得元
                </h2>
                {yt.live && <span className="tag">ライブ配信</span>}
              </div>
              <div className="download-source">
                <div className="download-thumb">
                  {yt.thumbnail ? (
                    <img
                      src={yt.thumbnail}
                      alt=""
                      width={480}
                      height={270}
                      referrerPolicy="no-referrer"
                    />
                  ) : (
                    <Film size={24} aria-hidden="true" />
                  )}
                </div>
                <div className="download-source__text">
                  <h3 className="download-source__title" title={yt.title}>
                    {yt.title}
                  </h3>
                  <dl className="download-dl">
                    {yt.uploader && (
                      <div>
                        <dt>チャンネル</dt>
                        <dd className="mono">{yt.uploader}</dd>
                      </div>
                    )}
                    <div>
                      <dt>再生時間</dt>
                      <dd className="mono">{formatDuration(yt.durationSec)}</dd>
                    </div>
                    <div>
                      <dt>再生回数</dt>
                      <dd className="mono">{formatCount(yt.viewCount)} 回</dd>
                    </div>
                  </dl>
                  <div className="download-marks">
                    <span className="tag">
                      {mode === "video" ? "MP4" : audioFormat.toUpperCase()}
                    </span>
                    <span className="tag">
                      {mode === "video"
                        ? `${quality === "best" ? "最高画質" : `${quality}p`} まで`
                        : "音声のみ"}
                    </span>
                    {mode === "video" && compress && (
                      <>
                        <span className="tag">{CODEC_LABELS[compressCodec]}</span>
                        <span className="tag">CRF {compressCrf}</span>
                      </>
                    )}
                  </div>
                </div>
              </div>
            </section>
          )}

          {probing && !yt && (
            <div className="empty">
              <span className="empty__title download-inline">
                <Loader2 size={15} className="spin" aria-hidden="true" />
                解析しています
              </span>
              <span>URL から動画の情報を取得しています。</span>
            </div>
          )}

          {result && (
            <section className="panel" aria-labelledby="download-result">
              <div className="panel__head">
                <h2 className="panel__title" id="download-result">
                  {compression ? "ダウンロードと圧縮が完了しました" : "ダウンロードが完了しました"}
                </h2>
                <span className="panel__meta mono">
                  {compression
                    ? reduction >= 0
                      ? `-${reduction}%`
                      : `+${Math.abs(reduction)}%`
                    : formatBytes(result.size)}
                </span>
              </div>
              <div className="table-wrap">
                <table className="table">
                  <thead>
                    <tr>
                      <th scope="col">項目</th>
                      <th scope="col">値</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr>
                      <td className="muted">ファイル</td>
                      <td className="download-file" title={result.file}>
                        {result.file.split(/[\\/]/).pop() ?? result.file}
                      </td>
                    </tr>
                    <tr>
                      <td className="muted">サイズ</td>
                      <td className="num">{formatBytes(result.size)}</td>
                    </tr>
                    <tr>
                      <td className="muted">所要時間</td>
                      <td className="num">{formatMs(result.elapsedMs)}</td>
                    </tr>
                    {compression && (
                      <>
                        <tr>
                          <td className="muted">圧縮前</td>
                          <td className="num">{formatBytes(compression.sizeBefore)}</td>
                        </tr>
                        <tr>
                          <td className="muted">圧縮後</td>
                          <td className="num">{formatBytes(compression.sizeAfter)}</td>
                        </tr>
                        <tr>
                          <td className="muted">削減</td>
                          <td className="num">
                            {saved >= 0 ? formatBytes(saved) : `+${formatBytes(-saved)}`}
                          </td>
                        </tr>
                        <tr>
                          <td className="muted">エンコーダー</td>
                          <td className="num">{compression.encoder}</td>
                        </tr>
                        {compression.media && (
                          <tr>
                            <td className="muted">解像度</td>
                            <td className="num">
                              {compression.media.width}×{compression.media.height}
                            </td>
                          </tr>
                        )}
                      </>
                    )}
                  </tbody>
                </table>
              </div>
              <div className="panel__foot">
                <div className="row download-actions">
                  <button
                    type="button"
                    className="btn"
                    onClick={() => openOutput(result.file).catch(() => undefined)}
                  >
                    <Film size={15} aria-hidden="true" />
                    開く
                  </button>
                  <button
                    type="button"
                    className="btn"
                    onClick={() => revealOutput(result.file).catch(() => undefined)}
                  >
                    <FolderOpen size={15} aria-hidden="true" />
                    フォルダを開く
                  </button>
                  <button
                    type="button"
                    className="btn btn--quiet"
                    onClick={() => {
                      setResult(null);
                      setUrl("");
                      setYt(null);
                    }}
                  >
                    別の URL
                  </button>
                </div>
              </div>
            </section>
          )}

          {!yt && !probing && !result && (
            <div className="empty">
              <span className="empty__title">URL を解析すると、ここに動画の情報が出ます</span>
            </div>
          )}

          <aside className="note">
            <span className="note__title">対応サイト</span>
            <span>DRM 保護されたコンテンツやライブ配信の録画は取得できません。</span>
            <span>
              取得時のみネットワークを使用します。保存後の変換・圧縮はオフラインです。
            </span>
          </aside>
        </div>

        <aside className="download__side">
          <section className="panel" aria-labelledby="download-settings">
            <div className="panel__head">
              <h2 className="panel__title" id="download-settings">
                ダウンロード設定
              </h2>
            </div>

            <div className="panel__body panel__body--split">
              <div className="cell">
                <span className="field__label" id="download-mode">
                  種類
                </span>
                <div className="seg" role="group" aria-labelledby="download-mode">
                  <button
                    type="button"
                    className="seg__btn"
                    aria-pressed={mode === "video"}
                    onClick={() => setMode("video")}
                    disabled={running}
                  >
                    動画
                  </button>
                  <button
                    type="button"
                    className="seg__btn"
                    aria-pressed={mode === "audio"}
                    onClick={() => setMode("audio")}
                    disabled={running}
                  >
                    音声
                  </button>
                </div>
              </div>

              {mode === "video" ? (
                <div className="cell">
                  <label className="field__label" htmlFor="download-quality">
                    画質
                  </label>
                  <select
                    id="download-quality"
                    className="input"
                    value={quality}
                    onChange={(event) => setQuality(event.target.value)}
                    disabled={running}
                  >
                    {QUALITIES.map((item) => (
                      <option key={item.value} value={item.value}>
                        {item.label}
                      </option>
                    ))}
                  </select>
                  <p className="field__note">
                    互換性を優先して H.264 / AAC の MP4 を選択します。
                  </p>
                </div>
              ) : (
                <div className="cell">
                  <span className="field__label" id="download-audio">
                    音声形式
                  </span>
                  <div className="seg" role="group" aria-labelledby="download-audio">
                    {AUDIO_FORMATS.map((item) => (
                      <button
                        key={item.value}
                        type="button"
                        className="seg__btn"
                        aria-pressed={audioFormat === item.value}
                        onClick={() => setAudioFormat(item.value as typeof audioFormat)}
                        disabled={running}
                      >
                        {item.label}
                      </button>
                    ))}
                  </div>
                  {audioHint && <p className="field__note">{audioHint}</p>}
                </div>
              )}

              <div className={mode === "audio" ? "cell cell--wide" : "cell"}>
                <label className="switch-row">
                  <span className="switch-row__text">
                    <span className="switch-row__title">サムネイルを埋め込む</span>
                  </span>
                  <span className="switch">
                    <input
                      type="checkbox"
                      checked={embedThumbnail}
                      onChange={(event) => setEmbedThumbnail(event.target.checked)}
                      disabled={running}
                    />
                    <span className="switch__knob" />
                  </span>
                </label>
                <label className="switch-row">
                  <span className="switch-row__text">
                    <span className="switch-row__title">メタデータを埋め込む</span>
                  </span>
                  <span className="switch">
                    <input
                      type="checkbox"
                      checked={embedMetadata}
                      onChange={(event) => setEmbedMetadata(event.target.checked)}
                      disabled={running}
                    />
                    <span className="switch__knob" />
                  </span>
                </label>
              </div>

              {mode === "video" && (
                <div className="cell">
                  <label className="switch-row">
                    <span className="switch-row__text">
                      <span className="switch-row__title">ダウンロード後に圧縮</span>
                      <span className="switch-row__sub">
                        取得した MP4 を FFmpeg で小さくします
                      </span>
                    </span>
                    <span className="switch">
                      <input
                        type="checkbox"
                        checked={compress}
                        onChange={(event) => setCompress(event.target.checked)}
                        disabled={running}
                      />
                      <span className="switch__knob" />
                    </span>
                  </label>
                  <p className="field__note">完了後に元ファイルは削除されます。</p>
                </div>
              )}

              {mode === "video" && compress && (
                <>
                  <div className="cell cell--wide">
                    <span className="field__label" id="download-crf">
                      圧縮の画質（CRF {compressCrf}）
                    </span>
                    <div className="seg" role="group" aria-labelledby="download-crf">
                      {QUALITY_PRESETS[compressCodec].map((preset) => (
                        <button
                          key={preset.crf}
                          type="button"
                          className="seg__btn"
                          aria-pressed={compressCrf === preset.crf}
                          onClick={() => setCompressCrf(preset.crf)}
                          disabled={running}
                          title={`CRF ${preset.crf}、${preset.hint}`}
                        >
                          {preset.label}
                        </button>
                      ))}
                    </div>
                  </div>

                  <div className={gpuAv1 ? "cell cell--wide" : "cell"}>
                    <span className="field__label" id="download-codec">
                      圧縮コーデック
                    </span>
                    <div className="seg" role="group" aria-labelledby="download-codec">
                      {(Object.keys(CODEC_LABELS) as Codec[])
                        .filter((item) => item !== "av1_gpu" || gpuAv1)
                        .map((item) => (
                        <button
                          key={item}
                          type="button"
                          className="seg__btn"
                          aria-pressed={compressCodec === item}
                          onClick={() => {
                            setCompressCodec(item);
                            setCompressCrf(DEFAULT_CRF[item]);
                          }}
                          disabled={running}
                          title={CODEC_NOTES[item]}
                        >
                          {CODEC_LABELS[item]}
                        </button>
                      ))}
                    </div>
                  </div>

                  <div className="cell">
                    <label className="field__label" htmlFor="download-height">
                      圧縮後の解像度
                    </label>
                    <select
                      id="download-height"
                      className="input"
                      value={compressHeight}
                      onChange={(event) => setCompressHeight(Number(event.target.value))}
                      disabled={running}
                    >
                      {RESOLUTIONS.map((res) => (
                        <option key={res.value} value={res.value}>
                          {res.label}
                        </option>
                      ))}
                    </select>
                  </div>
                </>
              )}

              <div className="cell cell--wide">
                <span className="field__label" id="download-dir">
                  保存先フォルダ
                </span>
                <button
                  type="button"
                  className="pick"
                  onClick={chooseOutputDir}
                  disabled={running}
                  title={outputDir}
                  aria-labelledby="download-dir"
                >
                  <FolderOpen size={15} aria-hidden="true" />
                  <span className="pick__text">
                    <span className="pick__value">{outputDir || "未選択"}</span>
                  </span>
                </button>
              </div>

              <div className="cell cell--wide">
                <label className="field__label" htmlFor="download-cookies">
                  Cookie（年齢制限など）
                </label>
                <select
                  id="download-cookies"
                  className="input"
                  value={cookies}
                  onChange={(event) => setCookies(event.target.value)}
                  disabled={running}
                >
                  {COOKIES.map((item) => (
                    <option key={item.value} value={item.value}>
                      {item.label}
                    </option>
                  ))}
                </select>
                <p className="field__note">
                  通常は「使用しない」で構いません。
                </p>
              </div>
            </div>

            <div className="panel__foot">
              <ProgressPanel
                embedded
                progress={progress}
                running={running}
                onCancel={cancel}
                label={compress && mode === "video" ? "ダウンロード / 圧縮中" : "ダウンロード中"}
              />
              <button
                type="button"
                className="btn btn--primary btn--block"
                onClick={start}
                disabled={running || !yt || !outputDir || !ytDlpReady || !ffmpegReady}
              >
                {running ? (
                  <>
                    <RefreshCw size={17} className="spin" aria-hidden="true" />
                    ダウンロード中…
                  </>
                ) : (
                  <>
                    <Download size={17} aria-hidden="true" />
                    {mode === "video"
                      ? compress
                        ? "ダウンロードして圧縮"
                        : "動画をダウンロード"
                      : "音声をダウンロード"}
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
