# Contributions back

Fixes found while building this launcher that the community lacks, drafted
for PCGamingWiki, upstream projects, ProtonDB or the WineHQ AppDB
(`sdk/docs/playbooks/contribute-back.md`). These are public posts under the
owner's name: agents only draft them here; the owner reviews and posts.

**Every fix of our own gets a row here**: each `docs/RESEARCH.md` row
decided as **Our own** cites its draft id (C1, C2, …), and each byte patch,
hook or preload library the launcher ships has one. A fix not verified in
the real game yet stays a draft with status "needs verification". When a
fix can't be shared, the RESEARCH.md row says `no draft: <reason>` instead.
`sdk/scripts/launcher-check` (rule `own-fix-draft`) checks this.

| Id | Finding | Target (page or project) | Evidence | Status |
|---|---|---|---|---|
| C1 | `--input-pipe` for windowsarcadeloader's launcher: a frontend feeds the virtual stick on stdin and reads the force feedback outputs on stdout | aderumier/windowsarcadeloader (pull request) | `patches/wal/0001`; races in SEGA Rally 3, SEGA Racing Classic, Daytona driven through it | draft |
| C2 | windowsarcadeloader's Windows host build (`crates/winhost`): its run on Windows itself, no Wine | aderumier/windowsarcadeloader (pull request) | `patches/wal/0003`; built for x86_64-pc-windows-gnu, not yet run on Windows | needs verification |
| C3 | Daytona Championship USA profile (`segashell`): the game's DYT Shell protocol, the InpWrapper force feedback helper, the security challenge switched off | aderumier/windowsarcadeloader (pull request) | `patches/wal/0004`; races verified headless (docs/RUNTIME_NOTES.md) | draft |

## Drafts

One section per id: the text ready to paste, the exact executable it
applies to (SHA-256, version), and a citation of this repository. Never
attach or link game files.

### C1, C2, C3: pull requests to windowsarcadeloader

One pull request each, from the patches as they are in `patches/wal/` (they
apply in order on commit `4f42c6d`):

> **Title:** Add `--input-pipe`: a frontend drives the game from stdin
>
> A launcher that reads the player's devices itself (wheels with a steering
> lock, pads with its own layout) can drive any game through
> `arcade-launcher run … --input-pipe`: it writes
> `p<N> <buttons hex> <lx> <ly> <rx> <ry> <accel> <brake>` lines, `quit`
> ends the game, and the game's outputs (force feedback, lamps) come back on
> stdout as `out <player> <id> <value>`. Implies `--no-input`. Used by
> https://github.com/akitaonrails/rea-teknoparrot-sega-series.

> **Title:** Daytona Championship USA (segashell)
>
> A profile for Daytona Championship USA New Season Edition: the Shell's
> `dyt` protocol (cabinet configuration and credits on launch), the Shell's
> `InpWrapper.exe` force feedback helper emulated (`daytona` byte format, the
> same as SEGA Rally 3's), and the Shell's security challenge switched off
> as for Transformers. Verified headless: attract, coins, start, menus and
> races with steering, gas and brake. Not verified: the view and gear
> buttons (races in automatic).

The Windows host (C2) waits for a run on real Windows before it is offered.
