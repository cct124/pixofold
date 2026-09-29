import { useLayoutEffect, useRef, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import styles from './FloatingPanel.module.css';

const controls = 'button:not(:disabled), a[href], input:not(:disabled), [tabindex="0"]';

/** 非模态浮层逃离表格裁切；无焦点陷阱，滚动来源行/点击外部/Escape可关闭。 */
export function FloatingPanel({
  id,
  label,
  anchor,
  onClose,
  onMouseEnter,
  onMouseLeave,
  children,
}: {
  id: string;
  label: string;
  anchor: RefObject<HTMLElement | null>;
  onClose: (restoreFocus: boolean) => void;
  onMouseEnter?: (() => void) | undefined;
  onMouseLeave?: (() => void) | undefined;
  children: ReactNode;
}) {
  const panel = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    const box = panel.current,
      trigger = anchor.current;
    if (!box || !trigger) return;
    const position = () => {
      const rect = trigger.getBoundingClientRect();
      const width = Math.min(360, Math.max(0, window.innerWidth - 24));
      box.style.width = width + 'px';
      box.style.maxHeight = Math.max(0, window.innerHeight - 24) + 'px';
      const height = box.getBoundingClientRect().height;
      box.style.left =
        Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12)) + 'px';
      box.style.top =
        Math.max(
          12,
          rect.bottom + 6 + height <= window.innerHeight - 12
            ? rect.bottom + 6
            : rect.top - height - 6,
        ) + 'px';
    };
    position();
    // 一个页面只保留一个非模态浮层，避免键盘焦点仍在上一行时悬停出第二层。
    const opened = (event: Event) => {
      if (event instanceof CustomEvent && event.detail !== id) onClose(false);
    };
    document.dispatchEvent(new CustomEvent('pixofold:popover-open', { detail: id }));
    document.addEventListener('pixofold:popover-open', opened);
    const resize = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(position);
    resize?.observe(box);
    const outside = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !box.contains(event.target) &&
        !trigger.contains(event.target)
      )
        onClose(false);
    };
    const scroll = (event: Event) => {
      if (event.target instanceof Node && box.contains(event.target)) return;
      onClose(false);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        onClose(true);
      } else if (event.key === 'Tab') {
        const focusable = [...box.querySelectorAll<HTMLElement>(controls)];
        if (!event.shiftKey && document.activeElement === trigger && focusable[0]) {
          event.preventDefault();
          focusable[0].focus();
        } else if (event.shiftKey && document.activeElement === focusable[0]) {
          event.preventDefault();
          trigger.focus();
        } else if (!event.shiftKey && document.activeElement === focusable.at(-1)) {
          // 回到触发位置后让浏览器继续自然Tab顺序，不能把用户困在portal尾部。
          onClose(false);
          trigger.focus();
        }
      }
    };
    const focus = (event: FocusEvent) => {
      if (
        event.target instanceof Node &&
        !box.contains(event.target) &&
        !trigger.contains(event.target)
      )
        onClose(false);
    };
    const modal = new MutationObserver(() => {
      if (document.querySelector('dialog[open]:not([data-floating-panel])')) onClose(false);
    });
    modal.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['open'],
    });
    window.addEventListener('resize', position);
    window.addEventListener('scroll', scroll, true);
    document.addEventListener('pointerdown', outside, true);
    document.addEventListener('keydown', key);
    document.addEventListener('focusin', focus);
    return () => {
      resize?.disconnect();
      modal.disconnect();
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', scroll, true);
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('keydown', key);
      document.removeEventListener('focusin', focus);
      document.removeEventListener('pixofold:popover-open', opened);
    };
  }, [anchor, id, onClose]);
  return createPortal(
    // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- 容器悬停仅延迟收起，允许移入阅读；实际操作用原生按钮，并单独支持/测试聚焦、Tab和Escape。
    <dialog
      id={id}
      ref={panel}
      open
      data-floating-panel
      aria-label={label}
      className={styles.panel}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      {children}
    </dialog>,
    document.body,
  );
}
