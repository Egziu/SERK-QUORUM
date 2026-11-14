# Design language

* Logo: the original four-node QUORUM emblem (`assets/quorum-logo.svg`), unchanged.
* Palette: obsidian `#0B0F0D`, deep forest `#123C2B`, intelligence green `#42C982`, white `#FFFFFF`, neutral gray `#8B9690`, surface gray `#181D1A`. Tokens in `apps/web/src/styles/app.css`.
* Themes: light (default) and dark, following the system setting with a manual override.
* Typography: system UI font stack (no external font requests); hierarchy through size and weight, not decoration.
* Principles: minimal, calm, precise; no gradients, heavy shadows or decorative charts; motion limited to a spinner and the mobile drawer, disabled under `prefers-reduced-motion`.

## Contrast (WCAG 2.2 AA, computed)

| Pair | Ratio |
| --- | --- |
| Light: text `#0B0F0D` on `#F7F9F8` | 18.25 |
| Light: muted `#55615A` on white | 6.47 |
| Light: accent `#1A6B47` on white | 6.49 |
| Light: white on primary forest `#123C2B` | 12.30 |
| Dark: white on obsidian | 19.29 |
| Dark: neutral gray on surface `#181D1A` | 5.58 |
| Dark: green `#42C982` on surface | 8.06 |
| Dark: obsidian on green button | 9.10 |

Brand neutral gray on white is 3.06 and is therefore used only for borders in the light theme, never for text.

## Accessibility practices
Skip link; labelled landmarks; every control labelled (`Field` component wires hint and error via `aria-describedby`/`aria-invalid`); visible focus rings; ARIA tabs with arrow-key navigation; keyboard-focusable scroll regions; status and alert live regions; 40px minimum control height. Verified with axe-core on 12 page states (light, dark, mobile).
