// SPDX-License-Identifier: GPL-2.0-only
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Search, X } from "lucide-react";
import type { Category, FormatInfo } from "../lib/api";
import { CATEGORY_ORDER, categoryLabel } from "../lib/format";
import { isAndroid } from "../lib/platform";
import { CategoryIcon } from "./CategoryIcon";

export function FormatPicker({
  open,
  formats,
  selected,
  onSelect,
  onClose,
  title = "出力形式を選択",
}: {
  open: boolean;
  formats: FormatInfo[];
  selected: string;
  onSelect: (id: string) => void;
  onClose: () => void;
  title?: string;
}) {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setCategory("all");
      // On phones, focusing the search box would cover the list with the keyboard.
      if (isAndroid) return;
      const timer = window.setTimeout(() => inputRef.current?.focus(), 30);
      return () => window.clearTimeout(timer);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const outputs = useMemo(() => formats.filter((f) => f.canOutput), [formats]);

  const counts = useMemo(() => {
    const map = new Map<string, number>();
    for (const f of outputs) map.set(f.category, (map.get(f.category) ?? 0) + 1);
    return map;
  }, [outputs]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return outputs.filter((format) => {
      if (category !== "all" && format.category !== category) return false;
      if (!q) return true;
      return (
        format.id.toLowerCase().includes(q) ||
        format.ext.toLowerCase().includes(q) ||
        format.label.toLowerCase().includes(q) ||
        (format.note ?? "").toLowerCase().includes(q)
      );
    });
  }, [outputs, query, category]);

  if (!open) return null;

  return (
    <div className="modal-overlay" onMouseDown={onClose} role="presentation">
      <div
        className="modal"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <div className="modal__head">
          <div>
            <h2>{title}</h2>
            <p>変換先の形式を選んでください。</p>
          </div>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="閉じる">
            <X size={16} />
          </button>
        </div>

        <div className="modal__search">
          <Search size={15} aria-hidden="true" />
          <input
            ref={inputRef}
            type="text"
            placeholder="形式を検索（例: png, mp4, pdf）"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label="形式を検索"
          />
          {query && (
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              onClick={() => setQuery("")}
              aria-label="検索をクリア"
            >
              <X size={13} />
            </button>
          )}
        </div>

        <div className="modal__tabs" role="group" aria-label="種別">
          <button
            type="button"
            className="modal__tab"
            aria-pressed={category === "all"}
            onClick={() => setCategory("all")}
          >
            すべて<span className="count num">{outputs.length}</span>
          </button>
          {CATEGORY_ORDER.filter((cat) => (counts.get(cat) ?? 0) > 0).map((cat) => (
            <button
              key={cat}
              type="button"
              className="modal__tab"
              aria-pressed={category === cat}
              onClick={() => setCategory(cat)}
            >
              {categoryLabel(cat)}
              <span className="count num">{counts.get(cat) ?? 0}</span>
            </button>
          ))}
        </div>

        <div className="modal__grid">
          {filtered.map((format) => (
            <button
              key={format.id}
              type="button"
              className={`format-option ${selected === format.id ? "is-selected" : ""}`}
              onClick={() => {
                onSelect(format.id);
                onClose();
              }}
            >
              <span className="format-option__top">
                <CategoryIcon category={format.category} size={14} />
                <span className="format-option__ext">.{format.ext}</span>
                {selected === format.id && (
                  <span className="format-option__check" aria-hidden="true">
                    <Check size={12} />
                  </span>
                )}
              </span>
              <span className="format-option__name">{format.label}</span>
              {format.note && <span className="format-option__note">{format.note}</span>}
            </button>
          ))}
          {filtered.length === 0 && (
            <div className="modal__empty">一致する形式がありません。</div>
          )}
        </div>
      </div>
    </div>
  );
}
