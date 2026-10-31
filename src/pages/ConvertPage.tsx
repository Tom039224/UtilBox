// SPDX-License-Identifier: GPL-2.0-only
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronRight,
  ExternalLink,
  FolderOpen,
  Loader2,
  RefreshCw,
  X,
} from "lucide-react";
import {
  api,
  errorMessage,
  listenJob,
  newJobId,
  type AppInfo,
  type ConvertItemResult,
  type ConvertResponse,
  type DetectedFile,
  type FormatInfo,
  type JobProgress,
  type ToolInfo,
} from "../lib/api";
import {
  formatBytes,
  formatMs,
  suggestTarget,
  categoryLabel,
} from "../lib/format";
import { isAndroid, openOutput, pickFolder, revealOutput } from "../lib/platform";
import { CategoryIcon } from "../components/CategoryIcon";
import { Dropzone } from "../components/Dropzone";
import { FormatPicker } from "../components/FormatPicker";
import { InlineAlert, ProgressPanel } from "../components/ProgressPanel";
import { PageHeader } from "../components/PageHeader";

export function ConvertPage({
  active,
  info,
  formats,
  tools,
}: {
  active: boolean;
  info: AppInfo | null;
  formats: FormatInfo[];
  tools: ToolInfo[];
}) {
  const [files, setFiles] = useState<DetectedFile[]>([]);
  const [target, setTarget] = useState("png");
  const [outputDir, setOutputDir] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [response, setResponse] = useState<ConvertResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const coreReady = ["ffmpeg", "ffprobe"].every((name) =>
    tools.some((tool) => tool.name === name && tool.available),
  );

  // Follow the default folder from settings until the user picks one here.
  const knownDefaultDir = useRef<string | null>(null);
  useEffect(() => {
    const next = info?.defaultOutputDir;
    if (!next) return;
    const previous = knownDefaultDir.current;
    knownDefaultDir.current = next;
    setOutputDir((current) => (!current || current === previous ? next : current));
  }, [info?.defaultOutputDir]);

  const selectedFormat = useMemo(
    () => formats.find((format) => format.id === target),
    [formats, target],
  );

  const addPaths = async (paths: string[]) => {
    if (paths.length === 0) return;
    setError(null);
    try {
      const detected = await api.detectFiles(paths);
      const valid = detected.filter((file) => file.size > 0);
      if (valid.length === 0) {
        setError("ファイルを読み込めませんでした。フォルダは指定できません。");
        return;
      }
      if (files.length === 0 && valid[0].formatId) {
        setTarget(suggestTarget(valid[0].category, valid[0].ext));
      }
      setFiles((prev) => {
        const seen = new Set(prev.map((file) => file.path));
        return [...prev, ...valid.filter((file) => !seen.has(file.path))];
      });
      setResponse(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const removeFile = (path: string) => {
    setFiles((prev) => prev.filter((file) => file.path !== path));
    setResponse(null);
  };

  const chooseOutputDir = async () => {
    try {
      const dir = await pickFolder("出力フォルダ");
      if (dir) setOutputDir(dir);
    } catch {
      /* cancelled */
    }
  };

  const startConvert = async () => {
    if (running || files.length === 0) return;
    if (!outputDir && !isAndroid) {
      setError("出力フォルダを指定してください。");
      return;
    }
    const id = newJobId("convert");
    setJobId(id);
    setRunning(true);
    setProgress(null);
    setResponse(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listenJob(id, { onProgress: setProgress });
      const res = await api.convertFiles(id, {
        inputs: files.map((file) => file.path),
        target,
        outputDir,
        overwrite,
      });
      setResponse(res);
      if (res.failed > 0) {
        setError(
          `${res.succeeded} 件成功、${res.failed} 件失敗しました。詳細は結果を確認してください。`,
        );
      }
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
        prev ? { ...prev, phase: "キャンセル", message: "処理を停止しています。" } : prev,
      );
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const totalBytes = files.reduce((sum, file) => sum + file.size, 0);
  const okCount = response?.succeeded ?? 0;

  return (
    <section className="sheet" hidden={!active}>
      <PageHeader
        title="ファイル変換"
        description={
          isAndroid
            ? "画像・動画・音声を端末内だけで変換します。アップロードはありません。"
            : "画像・動画・音声・文書・アーカイブをローカルのツールだけで変換します。アップロードはありません。"
        }
        meta={<span>{formats.length} 形式</span>}
      />

      <div className="stack alerts">
        {!coreReady && (
          <InlineAlert variant="warn">
            FFmpeg / FFprobe が見つかりません。「ツール設定」で診断を確認してください。
          </InlineAlert>
        )}
        {error && (
          <InlineAlert variant="error" onClose={() => setError(null)}>
            {error}
          </InlineAlert>
        )}
      </div>

      <div className="convert">
        <div className="convert__main">
          <Dropzone
            active={active}
            compact={files.length > 0}
            onFiles={addPaths}
            kind="media"
            title={isAndroid ? "変換したいファイルを選択" : "変換したいファイルをドロップ"}
            subtitle="複数ファイルをまとめて変換できます。"
            disabled={running}
          />

          <section className="panel" aria-labelledby="convert-files">
              <div className="panel__head">
                <h2 className="panel__title" id="convert-files">
                  ファイル
                  <span className="count num">{files.length}</span>
                </h2>
                <button
                  type="button"
                  className="btn btn--quiet btn--sm"
                  onClick={() => {
                    setFiles([]);
                    setResponse(null);
                  }}
                  disabled={running || files.length === 0}
                >
                  すべて削除
                </button>
              </div>
              <div className="table-wrap">
                <table className="table register-file">
                  <thead>
                    <tr>
                      <th scope="col">名前</th>
                      <th scope="col">種別</th>
                      <th scope="col">サイズ</th>
                      <th scope="col">形式</th>
                      <th scope="col">
                        <span className="visually-hidden">操作</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {files.length === 0 && (
                      <tr>
                        <td colSpan={5} className="muted">
                          {isAndroid ? "ファイルはまだありません。上の枠をタップして選んでください。" : "ファイルはまだありません。上の枠にドロップするか、クリックして選んでください。"}
                        </td>
                      </tr>
                    )}
                    {files.map((file) => (
                      <tr key={file.path}>
                        <td>
                          <span className="register-name">
                            <CategoryIcon category={file.category} />
                            <span title={file.path}>{file.name}</span>
                          </span>
                        </td>
                        <td className="muted">{categoryLabel(file.category)}</td>
                        <td className="num">{formatBytes(file.size)}</td>
                        <td>
                          <span className="tag">.{file.ext || "?"}</span>
                        </td>
                        <td className="cell-act">
                          <button
                            type="button"
                            className="icon-btn icon-btn--sm"
                            onClick={() => removeFile(file.path)}
                            disabled={running}
                            aria-label={`${file.name} を一覧から削除`}
                          >
                            <X size={14} />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {files.length > 0 && (
                <div className="register-total">
                  <span>合計 {files.length} 件</span>
                  <span className="num">{formatBytes(totalBytes)}</span>
                </div>
              )}
          </section>

          {response && response.results.length > 0 && (
            <section className="panel" aria-labelledby="convert-results">
              <div className="panel__head">
                <h2 className="panel__title" id="convert-results">
                  変換結果
                  <span className="count num">
                    {okCount}/{response.results.length}
                  </span>
                </h2>
                {!isAndroid && response.results.some((item) => item.output) && (
                  <button
                    type="button"
                    className="btn btn--quiet btn--sm"
                    onClick={() => {
                      const outputs = response.results
                        .map((item) => item.output)
                        .filter((value): value is string => Boolean(value));
                      if (outputs.length > 0) {
                        revealOutput(outputs).catch(() => {
                          if (outputDir) revealOutput(outputDir).catch(() => undefined);
                        });
                      }
                    }}
                  >
                    <FolderOpen size={14} />
                    フォルダを開く
                  </button>
                )}
              </div>
              {isAndroid && <p className="field__note save-note">保存先: Download/UtilBox</p>}
              <div className="table-wrap">
                <table className="table register-result">
                  <thead>
                    <tr>
                      <th scope="col">状態</th>
                      <th scope="col">出力</th>
                      <th scope="col">サイズ</th>
                      <th scope="col">所要時間</th>
                      <th scope="col">
                        <span className="visually-hidden">操作</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {response.results.map((item) => (
                      <ResultRow key={`${item.input}-${item.output ?? "fail"}`} item={item} />
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          )}
        </div>

        <aside className="convert__side">
          <section className="panel" aria-labelledby="convert-settings">
            <div className="panel__head">
              <h2 className="panel__title" id="convert-settings">
                変換設定
              </h2>
            </div>

            <div className="panel__body">
              <div className="cell">
                <span className="field__label" id="out-format">
                  出力形式
                </span>
                <button
                  type="button"
                  className="pick"
                  onClick={() => setPickerOpen(true)}
                  disabled={running}
                  aria-labelledby="out-format"
                >
                  <CategoryIcon category={selectedFormat?.category ?? "other"} />
                  <span className="pick__text">
                    <span className="pick__name">
                      {selectedFormat?.label ?? target.toUpperCase()}
                    </span>
                    <span className="pick__sub">.{selectedFormat?.ext ?? target}</span>
                  </span>
                  <ChevronRight size={15} aria-hidden="true" />
                </button>
                {selectedFormat?.note && <p className="field__note">{selectedFormat.note}</p>}
              </div>

              {isAndroid ? (
                <div className="cell">
                  <span className="field__label">保存先</span>
                  <p className="field__note">Download/UtilBox</p>
                </div>
              ) : (
              <div className="cell">
                <span className="field__label" id="out-dir">
                  出力フォルダ
                </span>
                <button
                  type="button"
                  className="pick"
                  onClick={chooseOutputDir}
                  disabled={running}
                  title={outputDir}
                  aria-labelledby="out-dir"
                >
                  <FolderOpen size={15} aria-hidden="true" />
                  <span className="pick__text">
                    <span className="pick__value">{outputDir || "未選択"}</span>
                  </span>
                </button>
              </div>
              )}

              <label className="switch-row">
                <span className="switch-row__text">
                  <span className="switch-row__title">同名ファイルを上書き</span>
                  <span className="switch-row__sub">オフの場合は連番を付けます</span>
                </span>
                <span className="switch">
                  <input
                    type="checkbox"
                    checked={overwrite}
                    onChange={(event) => setOverwrite(event.target.checked)}
                    disabled={running}
                  />
                  <span className="switch__knob" />
                </span>
              </label>
            </div>

            <div className="panel__foot">
              <button
                type="button"
                className="btn btn--primary btn--block"
                onClick={startConvert}
                disabled={running || files.length === 0 || (!outputDir && !isAndroid)}
              >
                {running ? (
                  <>
                    <Loader2 size={16} className="spin" aria-hidden="true" />
                    変換中
                  </>
                ) : (
                  <>
                    <RefreshCw size={16} aria-hidden="true" />
                    {files.length > 1 ? `${files.length} 件を変換` : "変換を開始"}
                  </>
                )}
              </button>
            </div>
          </section>

          <ProgressPanel
            progress={progress}
            running={running}
            onCancel={cancel}
            label="変換中"
          />

          <aside className="note">
            <span className="note__title">注記</span>
            <span>ファイルはこの端末から出ません。処理はすべてローカルで完結します。</span>
            {!isAndroid && <span>PDF から画像への変換は 1 ページ目のみです。</span>}
            <span>動画から音声を取り出す場合は MP3 / WAV / FLAC を選んでください。</span>
          </aside>
        </aside>
      </div>

      <FormatPicker
        open={pickerOpen}
        formats={formats}
        selected={target}
        onSelect={setTarget}
        onClose={() => setPickerOpen(false)}
      />
    </section>
  );
}

function ResultRow({ item }: { item: ConvertItemResult }) {
  return (
    <tr className={item.ok ? undefined : "result-row--failed"}>
      <td>
        <span className="result-mark">{item.ok ? "完了" : "失敗"}</span>
      </td>
      <td>
        <span className="result-name">
          <span title={item.input}>{item.name}</span>
          {item.ok && item.output && (
            <span className="muted">{item.output.split(/[\\/]/).pop()}</span>
          )}
          {!item.ok && <span className="result-message">{item.message}</span>}
        </span>
      </td>
      <td className="num">
        {item.ok
          ? `${formatBytes(item.sizeBefore)} → ${formatBytes(item.sizeAfter)}`
          : "—"}
      </td>
      <td className="num">{item.ok ? formatMs(item.elapsedMs) : "—"}</td>
      <td className="cell-act">
        {item.ok && item.output ? (
          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={() =>
              (isAndroid ? openOutput(item.output as string) : revealOutput(item.output as string)).catch(
                () => undefined,
              )
            }
            aria-label={isAndroid ? "出力ファイルを開く" : "出力ファイルを表示"}
          >
            {isAndroid ? <ExternalLink size={14} /> : <FolderOpen size={14} />}
          </button>
        ) : null}
      </td>
    </tr>
  );
}
