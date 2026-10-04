# ADR 0004: The 1.0 config.json stays the settings contract

A rewrite is invisible to the person using it only if their settings survive it, so
`%APPDATA%\DL-FOV-Fixer\config.json` keeps its location and its existing key names, including the
stored extra tweaks as ordered key and value pairs per section. The reader stays as forgiving as the
Python one: unknown keys are ignored, missing keys fall back to defaults, and a corrupt file is
replaced by defaults rather than refused, because a settings file is untrusted input and losing a
tweak list is better than refusing to start. A `schemaVersion` field is written from 2.0.0 on, and
its absence means version 1, which is what lets a later format change be detected instead of guessed.
The cost is that the on-disk shape is now a compatibility surface with a checked-in 1.0.0 fixture in
its tests, rather than something the C# types are free to define.
