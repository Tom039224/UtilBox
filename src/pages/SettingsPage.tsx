import { useState } from "react";
import { DownloadCloud, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import {
  api,
  errorMessage,
  isTauri,
  listenJob,
  newJobId,
  type AppInfo,
  type JobProgress,
  type ToolInfo,
} from "../lib/api";
import { isAndroid, openExternalUrl, pickFolder } from "../lib/platform";
import { InlineAlert, ProgressPanel } from "../components/ProgressPanel";
import { PageHeader } from "../components/PageHeader";

const CREDITS = [
  { name: "convert.to.it", url: "https://github.com/p2r3/convert", note: "発想の原点" },
  { name: "FFmpeg", url: "https://ffmpeg.org", note: "動画・音声" },
  { name: "ImageMagick", url: "https://imagemagick.org", note: "画像" },
  { name: "LibreOffice", url: "https://www.libreoffice.org", note: "文書" },
  { name: "Pandoc", url: "https://pandoc.org", note: "Markdown / EPUB" },
  { name: "7-Zip", url: "https://www.7-zip.org", note: "アーカイブ" },
  { name: "yt-dlp", url: "https://github.com/yt-dlp/yt-dlp", note: "YouTube" },
  { name: "Tauri", url: "https://tauri.app", note: "デスクトップ基盤" },
];

const ANDROID_HIDDEN_CREDITS = ["ImageMagick", "LibreOffice", "Pandoc", "7-Zip", "yt-dlp"];

const OFFLINE_STEPS = [
  {
    step: "01",
    title: "ファイルは端末内だけ",
    body: "アップロード・クラウド送信は行いません。変換はローカルの CLI ツールが担当します。",
  },
  {
    step: "02",
    title: "取得だけネットワーク",
    body: "YouTube の解析・ダウンロード時のみ通信します。それ以外は完全オフラインです。",
  },
  {
    step: "03",
    title: "ツールを同梱",
    body: "FFmpeg / Pandoc / yt-dlp はバイナリを同梱。システムのツールも自動検出します。",
  },
];

export function SettingsPage({
  active,
  info,
  tools,
  onRefresh,
  onChangeDefaultOutputDir,
}: {
  active: boolean;
  info: AppInfo | null;
  tools: ToolInfo[];
  onRefresh: () => Promise<void>;
  onChangeDefaultOutputDir: (dir: string | null) => Promise<void>;
}) {
  const [refreshing, setRefreshing] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [changingDir, setChangingDir] = useState(false);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const available = tools.filter((tool) => tool.available).length;
  const credits = isAndroid
    ? CREDITS.filter((credit) => !ANDROID_HIDDEN_CREDITS.includes(credit.name))
    : CREDITS;
  const ytDlp = tools.find((tool) => tool.name === "yt-dlp");

  const refresh = async () => {
    setRefreshing(true);
    setError(null);
    try {
      await onRefresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRefreshing(false);
    }
  };

  const chooseDefaultOutputDir = async () => {
    if (changingDir) return;
    setError(null);
    try {
      const dir = await pickFolder("既定の出力先を選択");
      if (!dir) return;
      setChangingDir(true);
      await onChangeDefaultOutputDir(dir);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setChangingDir(false);
    }
  };

  const resetDefaultOutputDir = async () => {
    if (changingDir) return;
    setError(null);
    setChangingDir(true);
    try {
      await onChangeDefaultOutputDir(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setChangingDir(false);
    }
  };

  const updateYtDlp = async () => {
    if (updating) return;
    const id = newJobId("update");
    setUpdating(true);
    setProgress(null);
    setUpdateMessage(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listenJob(id, { onProgress: setProgress });
      const result = await api.updateYtdlp(id);
      setUpdateMessage(result.message);
      await onRefresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      unlisten?.();
      setUpdating(false);
    }
  };

  return (
    <section className="sheet" hidden={!active}>
      <PageHeader
        title="ツール設定"
        description={
          isAndroid
            ? "同梱ツールの状態を確認できます。アプリの処理はすべて端末内で実行されます。"
            : "同梱ツールの状態を確認し、yt-dlp を最新版に更新できます。アプリの処理はすべてローカルで実行されます。"
        }
      />

      <div className="settings">
        <div className="stack alerts">
          {error && (
            <InlineAlert variant="error" onClose={() => setError(null)}>
              {error}
            </InlineAlert>
          )}
          {updateMessage && (
            <InlineAlert variant="success" onClose={() => setUpdateMessage(null)}>
              {updateMessage}
            </InlineAlert>
          )}
        </div>

        <section className="panel" aria-labelledby="settings-app">
          <div className="panel__head">
            <h2 className="panel__title" id="settings-app">
              アプリ情報
            </h2>
            <span className="panel__meta">{info ? `v${info.version}` : "—"}</span>
          </div>
          <div className="panel__body">
            <dl className="settings-spec">
              <div className="settings-spec__row">
                <dt className="settings-spec__key">バージョン</dt>
                <dd className="settings-spec__value mono">{info?.version ?? "—"}</dd>
              </div>
              <div className="settings-spec__row">
                <dt className="settings-spec__key">プラットフォーム</dt>
                <dd className="settings-spec__value mono">{info?.platform ?? "—"}</dd>
              </div>
              <div className="settings-spec__row">
                <dt className="settings-spec__key">アーキテクチャ</dt>
                <dd className="settings-spec__value mono">{info?.arch ?? "—"}</dd>
              </div>
              {isAndroid ? (
                <div className="settings-spec__row">
                  <dt className="settings-spec__key">保存先</dt>
                  <dd className="settings-spec__value mono">Download/UtilBox</dd>
                </div>
              ) : (
                <>
              <div className="settings-spec__row">
                <dt className="settings-spec__key">ダウンロード</dt>
                <dd className="settings-spec__value mono" title={info?.downloadsDir}>
                  {info?.downloadsDir ?? "—"}
                </dd>
              </div>
              <div className="settings-spec__row">
                <dt className="settings-spec__key">一時フォルダ</dt>
                <dd className="settings-spec__value mono" title={info?.tempDir}>
                  {info?.tempDir ?? "—"}
                </dd>
              </div>
              <div className="settings-spec__row">
                <dt className="settings-spec__key">既定の出力先</dt>
                <dd className="settings-spec__value settings-spec__value--action">
                  <span className="mono settings-spec__path" title={info?.defaultOutputDir}>
                    {info?.defaultOutputDir ?? "—"}
                  </span>
                  <span className="settings-spec__actions">
                    <button
                      type="button"
                      className="btn btn--quiet btn--sm"
                      onClick={chooseDefaultOutputDir}
                      disabled={changingDir || !isTauri}
                      title={isTauri ? undefined : "デスクトップ版でのみ変更できます"}
                    >
                      {changingDir ? "変更中…" : "変更"}
                    </button>
                    {info?.outputDirIsCustom && (
                      <button
                        type="button"
                        className="btn btn--quiet btn--sm"
                        onClick={resetDefaultOutputDir}
                        disabled={changingDir}
                      >
                        既定に戻す
                      </button>
                    )}
                  </span>
                </dd>
              </div>
                </>
              )}
            </dl>
            {!isAndroid && (
              <p className="settings-spec__note">
                変換・圧縮・ダウンロードの初期保存先です。各画面の保存先からも個別に変更できます。
              </p>
            )}
          </div>
        </section>

        <section className="panel" aria-labelledby="settings-tools">
          <div className="panel__head">
            <h2 className="panel__title" id="settings-tools">
              変換ツール
              <span className="count num">{available}/{tools.length}</span>
            </h2>
            <button
              type="button"
              className="btn btn--quiet btn--sm"
              onClick={refresh}
              disabled={refreshing}
            >
              <RefreshCw
                size={14}
                className={refreshing ? "spin" : undefined}
                aria-hidden="true"
              />
              再スキャン
            </button>
          </div>

          <div className="table-wrap">
            <table className="table settings-tools">
              <thead>
                <tr>
                  <th scope="col">名前</th>
                  <th scope="col">用途</th>
                  <th scope="col">状態</th>
                  <th scope="col">バージョン</th>
                  <th scope="col">パス</th>
                </tr>
              </thead>
              <tbody>
                {tools.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="muted">
                      ツールの情報はまだありません。
                    </td>
                  </tr>
                ) : (
                  tools.map((tool) => (
                    <tr key={tool.name}>
                      <td>
                        <span className="settings-tool-name">
                          <span className="cell-name" title={tool.label}>
                            {tool.label}
                          </span>
                          {tool.bundled && <span className="tag">同梱</span>}
                        </span>
                      </td>
                      <td className="settings-tool-purpose">{tool.purpose}</td>
                      <td
                        className={
                          tool.available ? "settings-state" : "settings-state settings-state--off"
                        }
                      >
                        {tool.available ? "使用可" : "未検出"}
                      </td>
                      <td
                        className="num settings-tool-version"
                        title={tool.version ?? ""}
                      >
                        {tool.version || "—"}
                      </td>
                      <td
                        className="settings-tool-path"
                        title={tool.path ?? ""}
                      >
                        {tool.path ? shortenPath(tool.path) : "—"}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

        </section>

        {!isAndroid && (
        <section className="panel settings-update" aria-labelledby="settings-update">
          <div className="panel__head">
            <h2 className="panel__title" id="settings-update">
              yt-dlp の更新
            </h2>
            <span className="count">{ytDlp?.version ?? "未検出"}</span>
          </div>
          <div className="panel__body">
            <p className="settings-update__desc">
              YouTube の仕様変更に追随するため、yt-dlp だけはアプリ内から最新版を取得できます。
            </p>
            <div className="settings-update__action">
              <button
                type="button"
                className="btn btn--primary"
                onClick={updateYtDlp}
                disabled={updating}
              >
                {updating ? (
                  <Loader2 size={16} className="spin" aria-hidden="true" />
                ) : (
                  <DownloadCloud size={16} aria-hidden="true" />
                )}
                {updating ? "更新中…" : "最新版に更新"}
              </button>
              <p className="field__note">
                更新時のみネットワークを使用し、バイナリはアプリのデータフォルダに保存されます。
              </p>
            </div>
          </div>
        </section>
        )}

        {!isAndroid && (
          <ProgressPanel progress={progress} running={updating} label="yt-dlp を更新中" />
        )}

        <div className="settings-lower">
          <section className="panel" aria-labelledby="settings-offline">
            <div className="panel__head">
              <h2 className="panel__title" id="settings-offline">
                オフライン設計
              </h2>
            </div>
            <div className="panel__body">
              <ol className="settings-steps">
                {OFFLINE_STEPS.map((item) => (
                  <li className="settings-step" key={item.step}>
                    <span className="settings-step__num mono">{item.step}</span>
                    <span className="settings-step__text">
                      <span className="settings-step__title">{item.title}</span>
                      <span className="settings-step__body">{item.body}</span>
                    </span>
                  </li>
                ))}
              </ol>
            </div>
          </section>

          <section className="panel" aria-labelledby="settings-credits">
            <div className="panel__head">
              <h2 className="panel__title" id="settings-credits">
                クレジット
              </h2>
              <span className="panel__meta">{credits.length} 件</span>
            </div>
            <div className="panel__body">
              <p className="settings-credits__desc">
                UtilBox は convert.to.it の「何でも変換できる」思想に着想を得た独立実装です。
                各ツールのライセンスは各プロジェクトに従います。
              </p>
              <ul className="settings-credits">
                {credits.map((credit) => (
                  <li key={credit.name}>
                    <button
                      type="button"
                      className="settings-credit"
                      onClick={() => openExternalUrl(credit.url).catch(() => undefined)}
                      disabled={isAndroid}
                    >
                      <span className="settings-credit__name">{credit.name}</span>
                      <span className="settings-credit__note">{credit.note}</span>
                      <ExternalLink
                        size={12}
                        aria-hidden="true"
                        className="settings-credit__mark"
                      />
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          </section>
        </div>
      </div>
    </section>
  );
}

function shortenPath(path: string): string {
  if (path.length <= 38) return path;
  const parts = path.split(/[\\/]/);
  return `…/${parts.slice(-2).join("/")}`;
}
