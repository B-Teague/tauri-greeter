# Tauri Greeter Themes

A theme is one CSS file, optionally with a picture beside it. The greeter
appends it after the built-in stylesheet (`dusk.css`, the same format), so
anything you declare wins. It starts on `nebula` and the "Theme" picker in the
status bar switches between the other stylesheets installed in
`/usr/share/tauri-greeter/themes/` without restarting. The picker is a preview:
it is not remembered, so the next greeter is back to `nebula`.

`/etc/tauri-greeter/theme.css` is the operator's own file. It is not an entry in
the picker; it is applied over whichever theme is showing, so it wins over all
of them.

## Install a theme

```bash
sudo install -Dm644 themes/midnight.css /etc/tauri-greeter/theme.css
```

Restart the greeter (`sudo systemctl restart tauri-greeter`) to see the change.

## Recolour

Override the custom properties from [`dusk.css`](dusk.css):

```css
:root {
  --backdrop: linear-gradient(160deg, #1b2735, #090a0f);
  --fg: #ffffff;          /* clock, date, user name */
  --fg-muted: #cccccc;    /* status bar */
  --field-bg: #ffffff;    /* password field */
  --field-fg: #1c1c1c;    /* password text */
  --focus: #ffffff;       /* focus ring */
  --error: #ffd0cc;       /* failed login message */
  --bar-bg: rgba(0, 0, 0, 0.35); /* status bar background */
}
```

## Background image

Put a `.png`, `.jpg` or `.webp` next to the stylesheet, same name:

```bash
sudo install -Dm644 mine.jpg /etc/tauri-greeter/theme.jpg
```

The greeter inlines it as `--backdrop: url(...) center / cover no-repeat`,
ahead of the theme's own CSS, so a theme that wants it placed differently just
restates `--backdrop`. A picture alone is a whole theme: the stylesheet beside
it may be missing.

It has to be a file the greeter can read, under 16 MB, and it travels into the
page as a data: URI -- a CSS `url()` naming a filesystem path resolves against
`tauri://`, not the disk, and silently draws nothing. An installed theme's
picture goes beside its stylesheet in `/usr/share/tauri-greeter/themes/`.

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
`clock`, `identity`, `prompt`, `error`, `actions` and `status` — the same six
as 0.1.0. Everything added since shares one of them rather than claiming a
seventh, so a theme written against the old list still lays out correctly.

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
| `.screen` | the grid container; also `.locked` when LightDM set its lock hint |
| `.clock` | `.time`, `.date` |
| `.identity` | `.avatar`, `.user` (the account `<select>`, or `.user.manual`, a text input, when the username has to be typed) |
| `.prompt` | `.password`, `.enter` |
| `.error` | everything the greeter says, as a stack of `<p>` |
| `.actions` | `.action`, plus `.suspend` / `.hibernate` / `.reboot` / `.shutdown` to reach one button |
| `.status` | `.host`, and the session, keyboard, language and theme `<select>` elements |

`.avatar` is an inline `<svg>` when the account has no picture and an `<img>`
carrying a `data:` URI when LightDM has one, so size it with a selector that
matches both and give `img.avatar` the shape you want it cropped to.

Inside `.error`:

| Class | What it is |
| --- | --- |
| `.failure` | the greeter's own verdict — `Authentication failed.` and the two unavailable cases |
| `.message` | one line PAM emitted, plus `.failed` when PAM marked it an error |
| `.countdown` | the autologin timer, while it is running |

Not every element is always present. The keyboard, language and theme pickers
are drawn only when there is more than one thing to pick; a power button
appears only if logind says it would perform that action; the guest and
"Other…" entries appear only when the seat's hints allow them. A theme must not
assume any of them exists — and must not hide `.error`, since a login that
cannot be completed is explained there and nowhere else.

Each power button carries its own `aria-label`, so hiding the visible
`.action span` for an icon-only rail is safe.

## Notes

- `dusk.css` is the default theme, built into the binary and installed
  alongside the others so the picker can go back to it. Two more worked
  examples ship with the greeter: `midnight.css` moves every element by
  rewriting the grid, and `nebula.css` adds an animated background.
- Keep contrast high enough to read at a glance.
- `.status` can hold five items on a machine with several keyboard layouts. It
  wraps by default; a theme that overrides its `display` should keep it able to.
- Animate `transform` and `opacity` only. The greeter renders in software, so
  animating a gradient's position, a blur radius or a colour repaints the whole
  screen every frame. Gate anything moving behind
  `@media (prefers-reduced-motion: reduce)`, as `nebula.css` does.
- Test at 1024x768 as well as your own resolution.
- The greeter has no network access, and the CSP blocks remote fonts and
  images silently. Local paths in `url()` do not work either; a background
  belongs beside the stylesheet, where the greeter inlines it for you.
- A theme is appended after the built-in stylesheet at equal specificity, so a
  plain selector wins. `!important` is never needed.
