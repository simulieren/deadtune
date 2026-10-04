# ADR 0005: Keep the surgical text merge, not a KeyValues round trip

Deadlock's `gameinfo.gi` is a KeyValues file, so a real parser that reads the file into a tree and
writes it back looks like the clean answer, and it is the wrong one here: a round trip reformats
everything, drops comments, and rewrites nested sub-blocks such as `rate` and `speaker_config`, and
a rewritten sub-block stops the game from launching. The port therefore keeps the Python approach,
which finds a named block by brace matching (honoring quotes and `//` comments so a brace inside a
string never shifts the depth count), updates an existing key in place, inserts a missing key at the
top of the block, and refuses to touch a key that exists as a sub-block. Everything outside the keys
the user asked for is preserved byte for byte, newlines included, and the detected line ending is
reused for inserted lines. The price is that the merge is text manipulation with regular expressions
rather than a model, which is why it is the one area covered by shared behavior vectors in
`contracts/vectors/` instead of hand-written tests per language.
