#!/usr/bin/env python3
"""Fold index.html into one self-contained file.

The site in this directory is an ordinary static site: index.html plus
shots/. Some hosts — Claude Artifacts among them — refuse every request to
an external host, so a page there has to carry its images inside it. This
inlines each <img src="shots/…"> as a data URI and drops the html/head/body
skeleton the host supplies itself.

    ./build-inline.py [out.html]
"""

import base64
import mimetypes
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
SRC = HERE / "index.html"


def inline_images(html: str) -> str:
    def sub(m: re.Match) -> str:
        attr, rel = m.group(1), m.group(2)
        path = HERE / rel
        if not path.exists():
            raise SystemExit(f"missing image: {rel}")
        mime = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        data = base64.b64encode(path.read_bytes()).decode("ascii")
        return f'{attr}="data:{mime};base64,{data}"'

    # `data-src` as well as `src`, and the attribute name is kept: the stage
    # defers its screenshots and swaps them in as the scroll reaches them.
    # Rewriting data-src to src here would hand the browser every screenshot
    # at once, which is the stall the deferral exists to avoid.
    return re.sub(r'\b(data-src|src)="((?:shots|assets)/[^"]+)"', sub, html)


def ascii_only(markup: str) -> str:
    """Escape non-ASCII as numeric entities, leaving <script> alone.

    The inlined file has no <head>, so it cannot carry a charset — the host
    supplies one and may not agree with us. Entities survive either way.
    Script bodies are exempt because HTML entities are not decoded inside
    them; they are asserted ASCII instead.
    """
    out = []
    for i, part in enumerate(re.split(r"(<script>.*?</script>)", markup, flags=re.S)):
        if i % 2:
            if not part.isascii():
                raise SystemExit("the inline <script> gained a non-ASCII character")
            out.append(part)
        else:
            out.append(part.encode("ascii", "xmlcharrefreplace").decode("ascii"))
    return "".join(out)


def main() -> None:
    out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "index.inline.html"
    html = SRC.read_text(encoding="utf-8")

    title = re.search(r"<title>(.*?)</title>", html, re.S)
    style = re.search(r"<style>(.*?)</style>", html, re.S)
    body = re.search(r"<body>(.*?)</body>", html, re.S)
    if not (title and style and body):
        raise SystemExit("index.html no longer has the shape this script expects")

    page = (
        f"<title>{ascii_only(title.group(1))}</title>\n"
        f"<style>{style.group(1)}</style>\n"
        f"{ascii_only(inline_images(body.group(1)))}\n"
    )
    out.write_text(page, encoding="utf-8")
    print(f"{out} — {len(page) / 1_048_576:.1f} MB")


if __name__ == "__main__":
    main()
