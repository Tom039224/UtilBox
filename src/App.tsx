// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { useCallback, useEffect, useRef, useState } from "react";
import { TopBar, type Page } from "./components/TopBar";
import { ConvertPage } from "./pages/ConvertPage";
import { CompressPage } from "./pages/CompressPage";
import { DownloadPage } from "./pages/DownloadPage";
import { SettingsPage } from "./pages/SettingsPage";
import { InlineAlert } from "./components/ProgressPanel";
import {
  api,
  errorMessage,
  type AppInfo,
  type FormatInfo,
  type ToolInfo,
} from "./lib/api";
import { isAndroid } from "./lib/platform";

const PAGES: Page[] = isAndroid
  ? ["convert", "compress", "settings"]
  : ["convert", "compress", "download", "settings"];

function pageFromHash(): Page {
  const raw = window.location.hash.replace(/^#\/?/, "");
  return PAGES.includes(raw as Page) ? (raw as Page) : "convert";
}

export default function App() {
  const [page, setPage] = useState<Page>(pageFromHash);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [formats, setFormats] = useState<FormatInfo[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [gpuAv1, setGpuAv1] = useState(false);
  const [bootError, setBootError] = useState<string | null>(null);
  const mainRef = useRef<HTMLElement>(null);

  const refreshTools = useCallback(async () => {
    const next = await api.listTools();
    setTools(next);
  }, []);

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const [toolList, formatList, appInfo] = await Promise.all([
          api.listTools(),
          api.listFormats(),
          api.appInfo(),
        ]);
        if (!mounted) return;
        setTools(toolList);
        setFormats(formatList);
        setInfo(appInfo);
      } catch (error) {
        if (mounted) setBootError(errorMessage(error));
      }
    })();
    // The GPU probe runs a test encode, so it must not delay startup.
    if (!isAndroid) {
      api
        .gpuAv1Available()
        .then((ok) => mounted && setGpuAv1(ok))
        .catch(() => undefined);
    }
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 });
  }, [page]);

  const changeDefaultOutputDir = useCallback(async (dir: string | null) => {
    const next = await api.setDefaultOutputDir(dir);
    setInfo(next);
  }, []);

  const navigate = useCallback((next: Page) => {
    if (window.location.hash !== `#${next}`) window.location.hash = next;
    setPage(next);
  }, []);

  const toolOk = tools.filter((tool) => tool.available).length;

  return (
    <div className="app">
      <TopBar
        page={page}
        onNavigate={navigate}
        toolOk={toolOk}
        toolTotal={tools.length || (isAndroid ? 2 : 9)}
      />
      <main className="main" ref={mainRef}>
        {bootError && (
          <div className="sheet sheet--flush">
            <InlineAlert variant="error" onClose={() => setBootError(null)}>
              バックエンドとの通信に失敗しました。{bootError}
            </InlineAlert>
          </div>
        )}
        <ConvertPage
          active={page === "convert"}
          info={info}
          formats={formats}
          tools={tools}
        />
        <CompressPage active={page === "compress"} info={info} tools={tools} gpuAv1={gpuAv1} />
        {!isAndroid && (
          <DownloadPage active={page === "download"} info={info} tools={tools} gpuAv1={gpuAv1} />
        )}
        <SettingsPage
          active={page === "settings"}
          info={info}
          tools={tools}
          onRefresh={refreshTools}
          onChangeDefaultOutputDir={changeDefaultOutputDir}
        />
      </main>
    </div>
  );
}
