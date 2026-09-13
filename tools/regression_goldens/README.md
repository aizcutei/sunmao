# Regression goldens

One trace per (plugin, format), produced by

```sh
sunmao_unittest_runner regress --write <trace> <plugin>
```

and checked against by

```sh
sunmao_unittest_runner regress --golden <trace> <plugin>
```

## What a trace pins

The seed, the sample rate, the maximum block size and the **uneven block
division** derived from the seed; the automation schedule; then per block the
peak, the RMS and four probe samples; then every parameter read back at the
end. See the module docs in `tools/sunmao_unittest_runner/src/regress.rs` for
why each of those is there.

## How it is compared

**Never for exact equality.** Each trace states its own tolerance in its header:

```
tolerance abs 9.99999999999999955e-7 rel 9.99999999999999955e-7
```

Both bounds are checked and either passing is enough — the absolute bound
carries values near zero where a relative bound means nothing, and the relative
bound carries large values where an absolute one would be unreasonable. The
comparison prints the **worst deviation it saw even when everything passed**, so
the margin is visible and a tolerance can be set from evidence rather than
taste.

Floats are written as `{:.17e}`: seventeen significant digits, which round-trips
an f64 exactly. The friendlier-looking `{:.17}` is seventeen *decimal places*
and renders `f32::MIN_POSITIVE` as `0.00000000000000000`, which would make a
golden record zero for every small sample and then match almost anything.

## Regenerating

A golden changes only when behaviour was **deliberately** changed. Regenerate
with `--write`, and make the diff part of the commit that changed the
behaviour — the diff is the evidence for what moved.

## What the two format traces say about each other

`SunMaoGain.vst3.trace` and `SunMaoGain.clap.trace` are identical in **every
record but the `format` line**, and CI asserts exactly that. The same plugin
driven by the same seed through two plugin formats produces the same audio and
ends on the same parameter values.

That was not true when these goldens were first written. VST3 then reported the
raw value the host had sent for the two *stepped* parameters while CLAP
reported the value the plugin had snapped to:

```
-final 2646080969 8.01757812500000000e-1     (VST3, the request)
+final 2646080969 1.00000000000000000e0      (both, the applied value)
```

The audio records were byte-identical even then, which is what proved the
plugin had quantised and only the VST3 readback had not. The fix is recorded in
`docs/phase5/progress.md`; the two-line diff above is what it looked like here,
and it is the reason these files are worth checking in.
