# git-miner
git-miner mines a commit SHA that has the given prefix.

It does so by inserting a series of invisible spaces at the end of the commit
message.

## Usage

This command produces a new commit object based on the latest commit, such
that it has a commit SHA that matches the given prefix:

```bash
git-miner --prefix 000000 --threads 8
```

Output:
```
Using 8 threads
Computed 31070000 hashes. Effective rate = 0.091 us per hash
Found after 31618507 tries!
Time taken: 2.89 s
Average time per hash: 0.091 us
000000f12710f3cf7b14b6585d0521e61702d4c5
```

This commit is now available in the git object database.

If you wish `git-miner` to replace the latest commit with this new commit
automatically, you can specify the `--amend` flag.

## Example
For kicks, this repository has decided to make its [commit
SHAs](https://github.com/YS-L/git-miner/commits/master) be always increasing
starting from 0000001 (when viewing just the first 7 bytes, of course). This
is the post commit hook:

```shell
#!/bin/bash

git-miner --prefix $(printf "%07d" $(git rev-list --count HEAD)) --threads=8 --amend
```

## Installation
Install from source:
```
cargo install --path $(pwd)
```