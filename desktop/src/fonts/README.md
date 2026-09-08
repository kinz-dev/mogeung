---
title: The bundled terminal font
status: active
updated: 2026-09-06
---

# The bundled terminal font

`MesloLGS NF`, carried in the bundle so the window's terminal renders an
agent's prompt the same way your own terminal does. `R-J89` has the full
reasoning; the short version is that **WebKit's WebContent process cannot see
`~/Library/Fonts`**, so a font you installed for yourself is invisible to the
window no matter what the CSS asks for.

## What is here

| File | Face | Size |
|---|---|---|
| `MesloLGS-NF-Regular.woff2` | regular, 400 | ~1.0 MB |
| `MesloLGS-NF-Bold.woff2` | bold, 700 | ~1.0 MB |

Italic is deliberately absent: WebKit synthesises it by slanting the regular
face, and a third megabyte for a style terminals barely use is not worth it.

Both are `woff2` converted from the `.ttf` with `fontTools`, which halves them
— 2.0 MB in the repository instead of 5.0 MB — and is the format every engine
we ship to reads. To regenerate from a `.ttf`:

```python
from fontTools.ttLib import TTFont
f = TTFont("MesloLGS NF Regular.ttf"); f.flavor = "woff2"
f.save("MesloLGS-NF-Regular.woff2")
```

## Provenance

Taken from the copy at `~/Library/Fonts` on the reporting machine, which is the
build [powerlevel10k](https://github.com/romkatv/powerlevel10k) distributes and
recommends.

The `name` table records:

- **Family** — `MesloLGS NF`
- **Version** — `Version 1.210;Nerd Fonts 2.3.3`
- **Copyright** — Copyright © 2009 Apple Inc. Copyright © 2006 by Tavmjong Bah.
  Copyright © 2003 by Bitstream, Inc. All Rights Reserved.

The lineage is Bitstream Vera → DejaVu → Apple's Menlo → André Berg's Meslo LG
→ patched with the Nerd Fonts glyph sets. Meslo LG is published under the
Apache License 2.0, and powerlevel10k distributes `MesloLGS NF` under it.

> **Not verified from the file.** The font's `name` table carries a copyright
> string but **no licence (id 13) or licence-URL (id 14) entry**, so the Apache
> 2.0 statement above comes from the upstream project's documentation rather
> than from these bytes. That is fine for a tool nobody has shipped yet, and it
> is the thing to check before mogeung is distributed to anyone else — along
> with whether to carry the upstream `LICENSE` text beside these files.

## Coverage, and why it is the point

Measured with `fontTools` on 2026-09-06, against the font the window was
actually falling back to:

| | codepoints | `U+E0B0` powerline | `U+F00C` Nerd icon | `❯` `✕` `✓` |
|---|---|---|---|---|
| `MesloLGS NF` | 13,828 | yes | yes | yes |
| Menlo | 2,759 | **no** | **no** | yes |

The last column is why the original report was confusing: the characters that
*did* work and the ones that did not were sitting next to each other in the
same prompt.
