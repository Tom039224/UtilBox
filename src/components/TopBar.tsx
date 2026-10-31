// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { isAndroid } from "../lib/platform";
import { ArrowLeftRight, Download, Minimize2, Wrench } from "lucide-react";

export type Page = "convert" | "compress" | "download" | "settings";

const ALL_NAV: { id: Page; label: string; short: string; Icon: typeof Wrench }[] = [
  { id: "convert", label: "変換", short: "変換", Icon: ArrowLeftRight },
  { id: "compress", label: "圧縮", short: "圧縮", Icon: Minimize2 },
  { id: "download", label: "ダウンロード", short: "取得", Icon: Download },
  { id: "settings", label: "ツール設定", short: "設定", Icon: Wrench },
];
const NAV = ALL_NAV.filter((item) => !isAndroid || item.id !== "download");

export function TopBar({
  page,
  onNavigate,
  toolOk,
  toolTotal,
}: {
  page: Page;
  onNavigate: (page: Page) => void;
  toolOk: number;
  toolTotal: number;
}) {
  return (
    <header className="topbar">
      <div className="topbar__inner">
        <div className="brand">
          <span className="brand__mark" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.6">
              <path d="M4 8V4h4M20 8V4h-4M4 16v4h4M20 16v4h-4" />
              <path d="M12 7v10M8 12h8" />
            </svg>
          </span>
          <span className="brand__name">UtilBox</span>
        </div>

        <nav className="tabs" aria-label="画面">
          {NAV.map(({ id, label, short, Icon }) => (
            <button
              key={id}
              type="button"
              className="tab"
              aria-current={page === id ? "page" : undefined}
              onClick={() => onNavigate(id)}
            >
              <Icon className="tab__icon" size={20} aria-hidden="true" />
              <span className="tab__label tab__label--full">{label}</span>
              <span className="tab__label tab__label--short">{short}</span>
            </button>
          ))}
        </nav>

        <div className="topbar__status">
          <span className="status-label">ローカル処理</span>
          <span className="divider" aria-hidden="true" />
          <span>
            <span className="status-word">ツール </span>
            <span className="num">{toolOk}/{toolTotal}</span>
          </span>
        </div>
      </div>
    </header>
  );
}
