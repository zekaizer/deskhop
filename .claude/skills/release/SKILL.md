---
name: release
description: Cut a DeskHop firmware release from the rust-port line — bump DDM_VERSION, build a release UF2, verify it, generate AI changelog notes vs the previous tag, and publish a GitHub release. Use when the user asks to "release", "cut a release", "publish a new firmware version", or "make a new vX.YY-ddmN".
---

# DeskHop Release

Cuts a firmware release from the **rust-port** line. Versions are `v{MAJOR}.{MINOR}-ddm{N}`;
each release bumps `DDM_VERSION` by 1. Release builds omit the debug serial / log ring.

## Workflow

Run the steps in order. Stop and report if any check fails — never publish a bad build.

### 1. Preflight
- [ ] On `rust-port`, working tree clean: `git status --short` (empty) and `git rev-parse --abbrev-ref HEAD`.
- [ ] Previous tag: `git tag --sort=-creatordate | grep -E '^v[0-9].*-ddm[0-9]+$' | head -1`.
- [ ] Read current version from `CMakeLists.txt` (`VERSION_MAJOR`, `VERSION_MINOR`, `DDM_VERSION`).
- [ ] Next tag = same MAJOR.MINOR, `DDM_VERSION + 1`. Confirm the computed tag with the user if ambiguous.

### 2. Bump version
- Edit only the `DDM_VERSION` line in `CMakeLists.txt` (+1).
- Commit on `rust-port`: `release: bump to v{MAJOR}.{MINOR}-ddm{N}` (no Co-Authored-By footer for a version bump is fine, but follow the repo convention).
- Push: `git push origin rust-port` (the release `--target` must exist on origin).

### 3. Release build (separate dir — preserves the dev `build/`)
```bash
cmake -B build-release -S .            # no -DDH_DEBUG, no -DDH_DEBUG_CDC_FLASH
cmake --build build-release -j$(nproc) 2>&1 | grep -iE "error|warning|Built target|FAILED"
```
Per-feature cargo target dirs mean no manual Rust-lib cleaning is needed.

### 4. Verify the artifact
```bash
python3 -c "import struct;d=open('build-release/deskhop.crc','rb').read();m,v,c=struct.unpack('<IHxxI',d[:12]);b=v//100;print(f'tag v{(b-100)//1000}.{(b-100)%1000}-ddm{v%100} raw={v} crc16=0x{c&0xFFFF:04x}')"
stat -c%s build-release/deskhop.bin   # MUST be exactly 262144
```
- [ ] Decoded tag matches the intended tag.
- [ ] `deskhop.bin` is exactly **262144** bytes.

### 5. Generate release notes
- Start from the commit log since the previous tag:
  `git log <prev_tag>..HEAD --no-merges --pretty='- %s'`
- Summarize into the template below. **Escalate when commit subjects are thin**: add
  `git log <prev_tag>..HEAD --stat` and, if still unclear, `git diff <prev_tag>..HEAD -- <path>`
  to write accurate Highlights. Group related commits; describe user-visible impact, not raw subjects.
- Keep it factual: don't invent features. Link `docs/` files with full
  `https://github.com/zekaizer/deskhop/blob/rust-port/...` URLs (see prior releases).
- Write the body to a temp file and pass it with `--notes-file` (avoids shell-escaping issues).

### 6. Publish
```bash
gh release create v{MAJOR}.{MINOR}-ddm{N} --repo zekaizer/deskhop --target rust-port \
  --title "v{MAJOR}.{MINOR}-ddm{N}" --notes-file /tmp/release-notes.md \
  build-release/deskhop.uf2
```
Attach the **release** `deskhop.uf2` (not a debug build). Report the release URL.

## Body template

```markdown
DeskHop Semi-DDM fork — rust-port line (FIRMWARE_VERSION <raw>).

## Highlights since <prev_tag>
- **<Theme>.** <User-visible description of the change and why it matters.>
- **<Theme>.** <...>

## Fixes
- <Bug fix, one line each.>   <!-- omit section if none -->

## Notes
- The attached `deskhop.uf2` is a **release build** (no debug serial / log ring).
- Flash one board; the second auto-syncs over the inter-board UART.
```

## Reference
- Tooling/process detail lives in the user's `release_process` memory and `CLAUDE.local.md`.
- Version encoding: `FIRMWARE_VERSION = (MAJOR*1000 + MINOR + 100)*100 + DDM_VERSION` (see `CMakeLists.txt`).
