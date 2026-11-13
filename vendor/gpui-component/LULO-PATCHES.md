# lulo patches to gpui-component 0.9.1

Copied from crates.io and wired in with `[patch.crates-io]` in the workspace `Cargo.toml`.
Every change is marked with a `// lulo:` comment. Keep this list current, and redo the
changes on a fresh copy when upgrading.

- `src/popover.rs`: dropdowns open in 200 ms on ease-out quint (transitions.dev "Menu
  dropdown" curve, `cubic-bezier(0.22, 1, 0.36, 1)`), instead of 150 ms ease-out cubic.
- `src/popover.rs`: every `Popover` (so every `dropdown_menu`) fades and slides in like
  `dropdown_popup` does; upstream only animated selects, comboboxes and date pickers, so
  button menus appeared at once.
- `src/menu/context_menu.rs`: context menus fade in rising 6px over 200 ms on ease-out quint,
  like the popovers above; upstream showed them at once.
- `src/notification.rs`: toasts enter and leave on Modrinth's timing (250 ms in, 200 ms out,
  56 px of travel, ease-out quint) and the stack reflows on the same 250 ms; upstream is
  Sonner's 400 ms over 96 px.
- `src/dialog/dialog.rs`: the dialog's close button shows the pointer cursor.
- `src/notification.rs`: a toast is a compact pill that hugs its text (status icon, one line;
  title plus muted detail when it has both), centered in the stack, dismissed by a click; the
  stack is a list of the newest three instead of Sonner's pile. Upstream is a fixed-width card
  with a hover close button.
- `src/notification.rs`: one-line toasts hide after 3 s, those with a title and detail after 5 s
  (upstream: 5 s for all).
- `src/menu/popup_menu.rs`: menu rows are 34 px tall with 0.75rem side padding and the menu is
  at least 12rem wide (upstream: 26 px, 0.5rem, 8rem).
- `src/menu/popup_menu.rs`: menu items show the pointer cursor.
