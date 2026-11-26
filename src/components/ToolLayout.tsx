import type { ReactNode } from "react";

/** Cabeçalho e área rolável comuns aos módulos de ferramentas. */
export function ToolPage({
  title,
  subtitle,
  actions,
  children,
}: {
  title: string;
  subtitle: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="tool">
      <header className="tool-head">
        <div>
          <h1>{title}</h1>
          <p className="muted">{subtitle}</p>
        </div>
        {actions && <div className="tool-actions">{actions}</div>}
      </header>
      <div className="tool-body">{children}</div>
    </div>
  );
}

export function AdminNotice({ elevated, onRelaunch, what }: { elevated: boolean; onRelaunch: () => void; what: string }) {
  if (elevated) return null;
  return (
    <div className="notice">
      <span>
        <strong>Sem privilégios de administrador.</strong> {what}
      </span>
      <button className="btn" onClick={onRelaunch}>
        Reabrir como administrador
      </button>
    </div>
  );
}

export function Toggle({
  checked,
  disabled,
  onChange,
  label,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`switch ${checked ? "on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="knob" />
    </button>
  );
}

export function Loading({ text }: { text: string }) {
  return (
    <div className="loading">
      <div className="spinner" aria-hidden />
      <span className="muted">{text}</span>
    </div>
  );
}
