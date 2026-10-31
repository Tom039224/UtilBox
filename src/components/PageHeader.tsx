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
