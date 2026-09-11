# CSSDM Themes

A theme is one CSS file. The greeter appends it after the built-in stylesheet,
so anything you declare wins. It loads `/etc/cssdm/theme.css` at startup, and
the "Theme" picker in the status bar switches between the stylesheets installed
in `/usr/share/cssdm/themes/` without restarting. The picker is a preview: it
is not remembered, so the next greeter is back to the operator's file.

## Install a theme

```bash
sudo install -Dm644 themes/midnight.css /etc/cssdm/theme.css
```

Restart the greeter (`sudo systemctl restart cssdm`) to see the change.

## Recolour

Override the custom properties from [`../styles.css`](../styles.css):

```css
:root {
  --backdrop: url("/usr/share/backgrounds/mine.jpg") center / cover no-repeat;
  --fg: #ffffff;          /* clock, date, user name */
  --fg-muted: #cccccc;    /* status bar */
  --field-bg: #ffffff;    /* password field */
  --field-fg: #1c1c1c;    /* password text */
  --focus: #ffffff;       /* focus ring */
  --error: #ffd0cc;       /* failed login message */
  --bar-bg: rgba(0, 0, 0, 0.35); /* status bar background */
}
```

## Resize

The controls size from properties too, so rescaling does not mean restating
their rules:

```css
:root {
  --avatar-size: 5.5rem;
  --field-height: 1.9rem;              /* password field and submit button */
  --field-width: min(17rem, 60vw);
  --field-radius: 3px;                 /* 999px for a pill */
  --icon-size: 1.6rem;                 /* power button glyphs */
}
```

## Move things around

`.screen` is one CSS grid and every element claims a named area, so a theme
repositions the whole greeter by rewriting a single declaration. The areas are
`clock`, `identity`, `prompt`, `error`, `actions` and `status`.

```css
.screen {
  grid-template-columns: 1fr auto;
  grid-template-rows: auto 1fr auto auto auto auto;
  grid-template-areas:
    "clock   actions"
    ".       ."
    ".       identity"
    ".       prompt"
    ".       error"
    "status  status";
}
```

That is the layout `midnight.css` uses: clock top-left, power buttons as an
icon rail on the right, login card in the bottom-right corner. No element
carries positioning of its own, so nothing has to be undone first.

## Restyle

Everything else is plain CSS against these classes:

| Class | What it is |
| --- | --- |
| `.screen` | the grid container |
| `.clock` | `.time`, `.date` |
| `.identity` | `.avatar` (an inline SVG), `.user` (the account `<select>`) |
| `.prompt` | `.password`, `.enter` |
| `.error` | failed-login message |
| `.actions` | `.action`, plus `.suspend` / `.reboot` / `.shutdown` to reach one button |
| `.status` | the session and theme `<select>` elements |

Each power button carries its own `aria-label`, so hiding the visible
`.action span` for an icon-only rail is safe.

## Notes

- Two worked examples ship with the greeter: `midnight.css` moves every
  element by rewriting the grid, and `nebula.css` adds an animated background.
- Keep contrast high enough to read at a glance.
- Animate `transform` and `opacity` only. The greeter renders in software, so
  animating a gradient's position, a blur radius or a colour repaints the whole
  screen every frame. Gate anything moving behind
  `@media (prefers-reduced-motion: reduce)`, as `nebula.css` does.
- Test at 1024x768 as well as your own resolution.
- The greeter has no network access, so reference only local files. Remote
  fonts and backgrounds are blocked by the CSP and fail silently.
- A theme is appended after the built-in stylesheet at equal specificity, so a
  plain selector wins. `!important` is never needed.
