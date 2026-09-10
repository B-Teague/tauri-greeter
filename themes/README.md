# CSSDM Themes

Customize the look and feel of CSSDM with CSS themes.

## Theme Structure

Each theme is a directory containing:

```
my-theme/
├── theme.json      (metadata)
└── theme.css       (styles)
```

## Creating a Custom Theme

### 1. Create a theme directory

```bash
mkdir -p ~/.local/share/cssdm/themes/my-theme
# or system-wide:
sudo mkdir -p /usr/share/cssdm/themes/my-theme
```

### 2. Create theme.json

```json
{
  "name": "My Theme",
  "description": "My custom CSSDM theme"
}
```

### 3. Create theme.css

Override CSS variables from the default theme:

```css
:root {
  /* Colors */
  --bg-primary: #1a1a1a;
  --bg-secondary: #2d2d2d;
  --text-primary: #ffffff;
  --text-secondary: #aaaaaa;
  --accent: #00d4ff;
  --accent-hover: #00a8cc;
  --input-bg: #333333;
  --input-border: #444444;
  --input-focus: #00d4ff;
  --error: #ff6b6b;
  --button-hover: #0099cc;

  /* Spacing & Design */
  --border-radius: 8px;
  --transition: all 0.2s ease;
  --shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  --shadow-hover: 0 8px 24px rgba(0, 212, 255, 0.2);
}

/* Optional: Add custom styles */
.login-box {
  border: 2px solid var(--accent);
}
```

## Theme Locations

Themes are loaded from:

1. **User themes** (highest priority): `~/.local/share/cssdm/themes/`
2. **System themes**: `/usr/share/cssdm/themes/`
3. **Built-in themes**: Default theme (bundled with CSSDM)

## CSS Variables Reference

### Colors

- `--bg-primary`: Main background
- `--bg-secondary`: Secondary background (cards, boxes)
- `--text-primary`: Primary text color
- `--text-secondary`: Secondary text (labels, hints)
- `--accent`: Accent color (buttons, highlights)
- `--accent-hover`: Accent on hover
- `--input-bg`: Input field background
- `--input-border`: Input field border
- `--input-focus`: Input focus color
- `--error`: Error message color
- `--button-hover`: Button hover color

### Design

- `--border-radius`: Border radius for elements
- `--transition`: CSS transition timing
- `--shadow`: Default shadow
- `--shadow-hover`: Hover shadow

## Example Themes

### Light Theme

See `light/theme.css` for a light theme example.

### High Contrast Theme

See `high-contrast/theme.css` for an accessible high-contrast theme.

## Tips

- Keep the same HTML structure (don't change class names)
- Override only the variables you need to customize
- Test your theme at multiple screen sizes
- Ensure sufficient color contrast for accessibility
- Use `prefers-color-scheme` for system preference detection

## Troubleshooting

- Theme not loading? Check the file paths and permissions
- Colors not changing? Make sure you're using CSS variable names correctly
- Theme selector showing up? Themes must have valid `theme.json` files
