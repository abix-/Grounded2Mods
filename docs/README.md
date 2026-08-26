# grounded2mods workspace docs

> **Authoritative on:** workspace-level concerns. Cross-cutting
> open work, chronology of milestones across all crates. Per-crate
> docs live next to the code in each crate's `docs/` folder.

## What's here

| File | Authoritative on |
|---|---|
| [todo.md](todo.md) | Open work across the workspace, ordered by leverage. |
| [changelog.md](changelog.md) | Chronological history of milestones, investigations, and design pivots across every crate. |
| [credits.md](credits.md) | Projects, tools, authors, and game studios credited by the workspace. |
| [decomp](../decomp/README.md) | Decompiler implementation, build instructions, sample output, and the retired Falcon prototype. |
| [decomp retrospective](../decomp/docs/retrospective.md) | Assessment of the decompilation work and whether further investment is worthwhile. |
| [wild-west-miner-research.md](wild-west-miner-research.md) | Research pass on modding Wild West Miner Simulator. ueforge vs Timberbot vs a hypothetical `unityforge`. |
| [unityforge-plan.md](unityforge-plan.md) | Extraction plan for `unityforge`: the C# sibling to ueforge that Goldbot will build on and Timberbot will port onto. |

## Per-crate docs

Each crate documents its own internals. Use the crate's `docs/`
folder when working on that crate.

| Crate | Path | Subject |
|---|---|---|
| ueforge | [`../ueforge/docs/`](../ueforge/docs/) | The UE4SS Rust mod framework. Subsystem reference, performance doctrine, research methodology. |
| grounded2-mod | [`../grounded2-mod/docs/`](../grounded2-mod/docs/) | Grounded 2 RPG / level-up mod. Damage internals, inventory, RPG subsystem, build/deploy, port history. |
| outworld-station-mod | [`../outworld-station-mod/`](../outworld-station-mod/) | Outworld Station mod. Research notes, per-feature tweaks. |

## Adding a new mod

When you add a new game mod to the workspace:

1. Create the crate dir (`<game>-<feature>/` or `<game>/<crate>/`).
2. Add a `docs/` folder there with a `README.md` index for the
   crate.
3. Per-feature deep-dives go in that `docs/` folder.
4. Update the table above so the workspace index lists it.
5. Cross-cutting open work for the new mod lives in this
   workspace `todo.md`.

## Doc rules

- **Co-locate.** A doc lives in the same crate as the code it
  documents. The exception is this directory. Only for
  workspace-spanning concerns (todo, changelog).
- **Authoritative + dated.** Every doc states what it owns at the
  top. Updates note their date.
- **Update with the change.** When you change behavior, update
  the doc in the same commit. Stale docs lie.
