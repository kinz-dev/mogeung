#!/usr/bin/env bash
# Lint the doc system: frontmatter, staleness, stray root docs.
#
# Staleness is the interesting check. A design doc declares `covers:` listing the
# code it describes; if any covered path has a commit newer than the doc's
# `updated:` date, the doc has drifted from its code and says so out loud.
#
# This is roadmap item R-H2 (staleness detection) built for ourselves first, at toy
# scale. Read it before building the real thing.

set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
warn=0

say()  { printf '%s\n' "$*"; }
err()  { printf '  \033[31mFAIL\033[0m %s\n' "$*"; fail=$((fail + 1)); }
note() { printf '  \033[33mWARN\033[0m %s\n' "$*"; warn=$((warn + 1)); }
ok()   { printf '  \033[32m ok \033[0m %s\n' "$*"; }

# --- 1. no markdown at the repository root except the allowed few -------------
say ""
say "Root markdown files"
allowed="AGENTS.md CLAUDE.md README.md CHANGELOG.md STATUS.md"
for f in *.md; do
    [ -e "$f" ] || continue
    case " $allowed " in
        *" $f "*) ok "$f" ;;
        *) err "$f — no markdown at the repo root; see docs/README.md" ;;
    esac
done

# --- 2. frontmatter -----------------------------------------------------------
say ""
say "Frontmatter"
missing=0
while IFS= read -r doc; do
    if [ "$(head -c 4 "$doc")" != "---" ]; then
        err "$doc — no frontmatter"
        missing=$((missing + 1))
        continue
    fi
    grep -q '^status:'  "$doc" || { err "$doc — missing 'status:'";  missing=$((missing+1)); }
    grep -q '^updated:' "$doc" || { err "$doc — missing 'updated:'"; missing=$((missing+1)); }
done < <(find docs -name '*.md' -not -path 'docs/_templates/*' -not -path 'docs/archive/*')
[ "$missing" -eq 0 ] && ok "all docs carry status and updated"

# --- 3. staleness: has covered code changed since the doc was updated? --------
say ""
say "Staleness (design docs vs the code they cover)"
stale=0
for doc in docs/design/*.md; do
    [ -e "$doc" ] || continue
    updated=$(grep -m1 '^updated:' "$doc" | sed 's/updated:[[:space:]]*//' | tr -d '"')
    [ -n "$updated" ] || continue

    # Read the `covers:` block: subsequent lines starting with "  - ".
    covers=$(awk '/^covers:/{f=1;next} f&&/^[[:space:]]*-[[:space:]]/{sub(/^[[:space:]]*-[[:space:]]*/,"");print;next} f&&/^[^[:space:]]/{exit}' "$doc")
    if [ -z "$covers" ]; then
        note "$doc — no 'covers:' block, staleness cannot be checked"
        continue
    fi

    drifted=""
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        if [ ! -e "$path" ]; then
            err "$doc — covers '$path' which no longer exists"
            continue
        fi
        # Any commit touching this path strictly after the doc's updated date?
        newer=$(git log --since="$updated 23:59:59" --format=%h -1 -- "$path" 2>/dev/null)
        [ -n "$newer" ] && drifted="$drifted $path"
    done <<< "$covers"

    if [ -n "$drifted" ]; then
        note "$doc (updated $updated) — code changed since:$drifted"
        stale=$((stale + 1))
    else
        ok "$(basename "$doc")"
    fi
done
[ "$stale" -eq 0 ] && ok "no design doc has drifted from its code"

# --- 4. broken relative links -------------------------------------------------
say ""
say "Links"
broken=$(python3 - <<'PY'
import pathlib, re
bad = []
for p in pathlib.Path('.').rglob('*.md'):
    if 'target' in p.parts or '.git' in p.parts:
        continue
    for m in re.finditer(r'\]\(([^)#]+\.md)[^)]*\)', p.read_text()):
        link = m.group(1)
        if link.startswith(('http', '#')):
            continue
        if not (p.parent / link).resolve().exists():
            bad.append(f"{p} -> {link}")
print("\n".join(bad))
PY
)
if [ -n "$broken" ]; then
    while IFS= read -r line; do err "broken link: $line"; done <<< "$broken"
else
    ok "all relative markdown links resolve"
fi

# --- 5. roadmap ids resolve ---------------------------------------------------
#
# Roadmap items are `R-A1`, assumptions are bare `A1`. They collided before
# 2026-07-25 and "A1 is untested" meant two different things depending on which
# file you had open. This check keeps the prefixed ids honest: every R-… mentioned
# anywhere must name a row that actually exists in the roadmap table.
say ""
say "Roadmap ids"
roadmap=docs/product/roadmap.md
known=$(grep -oE '^\| R-[A-Z][0-9]+ \|' "$roadmap" 2>/dev/null | tr -d '|' | tr '\n' ' ')
dangling=0
while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    file=${ref%%:*}
    id=${ref#*:}
    case " $known " in
        *" $id "*) ;;
        *) err "$file references '$id', which is not a roadmap item"
           dangling=$((dangling + 1)) ;;
    esac
done < <(grep -roE --include='*.md' --include='*.sh' \
             --exclude-dir=archive --exclude-dir=.git --exclude-dir=target \
             'R-[A-Z][0-9]+' . 2>/dev/null \
         | sort -u)
[ "$dangling" -eq 0 ] && ok "every R-… reference resolves ($(printf '%s' "$known" | wc -w | tr -d ' ') items)"

# --- 6. orphans: ADRs and features nobody links to ----------------------------
say ""
say "Cross-references"
for adr in docs/decisions/*.md; do
    [ -e "$adr" ] || continue
    base=$(basename "$adr")
    if ! grep -rq --include='*.md' --exclude="$base" "$base" . ; then
        note "$base — not referenced anywhere"
    fi
done

# --- summary ------------------------------------------------------------------
say ""
if [ "$fail" -gt 0 ]; then
    printf '\033[31m%d failure(s)\033[0m, %d warning(s)\n' "$fail" "$warn"
    exit 1
fi
printf '\033[32mDocs OK\033[0m — %d warning(s)\n' "$warn"
