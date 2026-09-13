# Agent instructions

The working rules for this repository live in **[`CLAUDE.md`](CLAUDE.md)** —
one file, so the rules cannot drift apart between agents. Read it before
touching anything.

They are not style preferences. This is a plug-in framework loaded into other
people's audio processes, so the constraints there are load-bearing:

- **The audio callback allocates and locks nowhere on the success path.**
- **A host-facing capability lands on VST3 and CLAP together**, or its
  degradation is written down with the test name that proves it.
- **Acceptance means one commit green on macOS, Windows, and Linux hosted
  jobs, with downloadable artifacts.** Local results are development evidence.
- **A green job is not evidence that an assertion ran.** This repository has
  twice had a fully green run in which the assertion under judgement executed
  zero times. Verify by finding it in the raw logs.
- **Troubleshoot bottom-up**: `_sys` bindings against the upstream headers
  first, then `_rs` ABI behaviour, and only then the `sunmao` layer.

Start from the current phase's `docs/phase<N>/status.md` and `progress.md`;
`docs/roadmap.md` has the direction, and each phase's `loop_prompt.md` the
goal it was driven by.
