// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

import type { ReactNode } from "react";

export function PageHeader({
  title,
  description,
  meta,
}: {
  title: string;
  description: string;
  meta?: ReactNode;
}) {
  return (
    <header className="page-head">
      <div>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {meta ? <div className="page-head__side">{meta}</div> : null}
    </header>
  );
}
