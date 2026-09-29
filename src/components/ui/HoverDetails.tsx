import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { FloatingPanel } from './FloatingPanel';

/** 悬停/聚焦即开；短暂跨越间隙不闪退，Escape后不会在同一次悬停中重新弹出。 */
export function HoverDetails({
  label,
  disabled,
  children,
}: {
  label: string;
  disabled: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dismissed = useRef(false);
  const id = useId();
  const cancel = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }, []);
  useEffect(() => cancel, [cancel]);
  const close = useCallback(
    (restoreFocus: boolean) => {
      cancel();
      dismissed.current = true;
      setOpen(false);
      if (restoreFocus) anchor.current?.focus();
    },
    [cancel],
  );
  const show = () => {
    cancel();
    if (
      !disabled &&
      !dismissed.current &&
      !document.querySelector('dialog[open]:not([data-floating-panel])')
    )
      setOpen(true);
  };
  const leave = () => {
    cancel();
    timer.current = setTimeout(() => {
      if (
        document.activeElement === anchor.current ||
        document.getElementById(id)?.contains(document.activeElement)
      )
        return;
      setOpen(false);
    }, 150);
  };
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="text-button"
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open && !disabled}
        aria-controls={open ? id : undefined}
        onMouseEnter={() => {
          dismissed.current = false;
          show();
        }}
        onMouseLeave={() => {
          dismissed.current = false;
          leave();
        }}
        onFocus={show}
        onBlur={() => {
          dismissed.current = false;
          leave();
        }}
        onClick={() => {
          dismissed.current = false;
          show();
        }}
      >
        {label}
      </button>
      {open && !disabled && (
        <FloatingPanel
          id={id}
          label={label}
          anchor={anchor}
          onClose={close}
          onMouseEnter={cancel}
          onMouseLeave={leave}
        >
          {children}
        </FloatingPanel>
      )}
    </>
  );
}
