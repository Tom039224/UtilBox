// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { AlertCircle, CheckCircle2, Info, Loader2, X } from "lucide-react";
import type { JobProgress } from "../lib/api";

export function ProgressPanel({
  progress,
  running,
  onCancel,
  label = "処理中",
  embedded = false,
}: {
  progress: JobProgress | null;
  running: boolean;
  onCancel?: () => void;
  label?: string;
  /** Render without its own tile so it can sit inside another panel's footer. */
  embedded?: boolean;
}) {
  if (!running && !progress) return null;
  const percent = Math.max(0, Math.min(100, progress?.percent ?? 0));
  const title = (
    <span className="panel__title">
      {running ? <Loader2 size={15} className="spin" aria-hidden="true" /> : <CheckCircle2 size={15} aria-hidden="true" />}
      {progress?.phase || label}
    </span>
  );
  const body = (
    <>
    <div
      className="progress__track"
      role="progressbar"
      aria-valuenow={Math.round(percent)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
    >
      <div className="progress__fill" style={{ transform: `scaleX(${percent / 100})` }} />
    </div>
    <p className="progress__msg">{progress?.message || "準備しています。"}</p>
    {progress?.detail && <p className="progress__detail">{progress.detail}</p>}
    {running && onCancel && (
      <button type="button" className="btn btn--quiet btn--sm" onClick={onCancel}>
        キャンセル
      </button>
    )}
    </>
  );
  if (embedded) {
    return (
      <div className="progress progress--embedded" aria-live="polite">
        <div className="progress__head">
          {title}
          <span className="count">{Math.round(percent)}%</span>
        </div>
        {body}
      </div>
    );
  }
  return (
    <section className="panel" aria-live="polite">
      <div className="panel__head">
        {title}
        <span className="count">{Math.round(percent)}%</span>
      </div>
      <div className="panel__body panel__body--tight">{body}</div>
    </section>
  );
}

export function InlineAlert({
  variant = "error",
  children,
  onClose,
}: {
  variant?: "error" | "info" | "success" | "warn";
  children: React.ReactNode;
  onClose?: () => void;
}) {
  const Icon =
    variant === "error" || variant === "warn"
      ? AlertCircle
      : variant === "success"
        ? CheckCircle2
        : Info;
  return (
    <div className={`alert alert--${variant}`} role="status">
      <Icon size={16} aria-hidden="true" />
      <div>{children}</div>
      {onClose && (
        <button type="button" className="icon-btn icon-btn--sm alert__close" onClick={onClose} aria-label="閉じる">
          <X size={14} />
        </button>
      )}
    </div>
  );
}
