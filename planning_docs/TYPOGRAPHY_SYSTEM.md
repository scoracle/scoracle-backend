# Scoracle Typography System

Design spec for the Scoracle frontend typography stack. This document is owned
by the data repo so the frontend repo (Astro) can pull from a single source of
truth. When the frontend adopts changes below, mirror the tokens into
`src/layouts/BaseLayout.astro` and the global stylesheet.

## Fonts

| Role              | Font          | Source       | Cost                |
|-------------------|---------------|--------------|---------------------|
| Headers / Display | **Tan Nimbus**| Fontshare    | Free, commercial OK |
| Body / UI         | **DM Sans**   | Google Fonts | Free                |

## Embed

```html
<!-- In <head> -->
<link href="https://api.fontshare.com/v2/css?f[]=tan-nimbus@400&display=swap" rel="stylesheet">
<link href="https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;400;500;600&display=swap" rel="stylesheet">
```

## CSS Tokens

```css
:root {
  --font-display: 'Tan Nimbus', serif;
  --font-body: 'DM Sans', sans-serif;

  --weight-light: 300;
  --weight-regular: 400;
  --weight-medium: 500;
  --weight-semibold: 600;
}

h1, h2, h3, h4 {
  font-family: var(--font-display);
  font-weight: 400; /* Tan Nimbus is a single-weight display face */
}

body, p, ui, label, button {
  font-family: var(--font-body);
  font-weight: 300; /* DM Sans Light is the workhorse weight */
}
```

## Usage Notes

- **Tan Nimbus** is a single-weight face — don't apply bold or heavy weights, it
  won't render correctly. Let size and tracking do the work instead.
- **Tracking on Tan Nimbus headers**: slight positive tracking (0.02–0.06em)
  reads well at display sizes. Tighten to -0.01em at very large (60px+) sizes.
- **DM Sans** at `font-weight: 300` (Light) is the primary body weight. Use 400
  for UI labels and 500–600 for CTAs and metadata only.
- Eyebrows / metadata labels: DM Sans 500, `letter-spacing: 0.12–0.16em`,
  `text-transform: uppercase`, muted color.

## Aesthetic Context

The font system was chosen to complement the primary logo: a tattoo flash-style
crystal ball illustration (thin line, no fill, slightly occult/mystical). Tan
Nimbus was selected because its line weight and ethereal quality match the
illustration's stroke energy. DM Sans provides grounded, warm contrast without
fighting the logo.

Target aesthetic: midpoint between Anthropic (warm, human) and NYT/The Athletic
(minimal, editorial).

## Astro Integration

If using Astro with SSR on Cloudflare Workers, load fonts in your base layout
(`src/layouts/BaseLayout.astro`):

```astro
---
// BaseLayout.astro
---
<html>
  <head>
    <link rel="preconnect" href="https://api.fontshare.com">
    <link href="https://api.fontshare.com/v2/css?f[]=tan-nimbus@400&display=swap" rel="stylesheet">
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;400;500;600&display=swap" rel="stylesheet">
  </head>
  <body>
    <slot />
  </body>
</html>
```

## Applying the Tokens

Drop the `:root` custom properties and the base `h1..h4` / `body` rules into
your global stylesheet (e.g. `src/styles/global.css`) and import it from
`BaseLayout.astro`. Any component that needs to override a font should reference
the tokens rather than hard-coding font family names, so future swaps touch only
this file.
