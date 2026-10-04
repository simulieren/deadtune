# Behavior vectors

These files describe what the app must do, as data. The Core tests (`tests/DlFovFixer.Core.Tests`)
read them, and no test keeps its own copy. Until 2.0.0 the Python 1.x app ran the same merge, tweak
and FOV files, which is how the C# port was shown to behave exactly like it.

| File | What it covers |
|---|---|
| `gameinfo-merge.json` | Merging keys into one named block: update in place, insert, skip a sub-block, create the block |
| `gameinfo-apply.json` | Applying the FOV value and the stored tweaks to a whole `gameinfo.gi` |
| `video-config.json` | Merging `setting.*` entries into `cfg/video.txt`, including creating it |
| `tweak-parsing.json` | Sorting a pasted config into ConVars, SceneSystem and video entries, and merging stored lists |
| `fov-value.json` | Which typed values are accepted, the FOV in degrees, and the menu presets |
| `semantic-version.json` | Reading and ordering versions, and which release the app is offered |
| `release-channel.json` | The update channel's addresses, which artifact paths may be downloaded, and which manifests are accepted |

## Rules every runner follows

- A text field is an array of lines joined with LF. `null` means no text, for example a file that
  does not exist.
- Every case in `gameinfo-merge.json`, `gameinfo-apply.json` and `video-config.json` that has text
  input also runs with each LF replaced by CRLF, in the input and in the expected output.
- Every one of those cases then runs a second time on its own output, and the second run must change
  nothing.
- Result names are the ones 1.x used: `updated`, `added`, `skipped_block`, `no_block`, `no_root`.

## Adding a case

Write the input by hand, and say in the name what the case proves. Work out the expected output,
then read it line by line before committing it: the vector is the contract, so a wrong expectation
locks in a bug. A new file needs a runner, and `VectorCoverageTests` fails when a file has none.

Some expectations record today's behavior rather than an ideal one, and a change to them is a
change to what the app writes:

- An inserted key is followed by a blank line, because the inserted lines end with a line break and
  the block's first line already starts with one.
- A created block leaves the root's closing brace indented by one tab.
- A `video.txt` with no block at all is replaced by a fresh block. The one-time backup keeps the
  original.
