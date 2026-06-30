# Argus demo — record with VHS (https://github.com/charmbracelet/vhs):
#   vhs benchmarks/demo.tape   ->   produces benchmarks/demo.gif
#
# Requires: target/debug/argus built from this repo.
# This records the money shot: the agent claims done, the verification gate
# catches the failing test, and the agent fixes it for real.

Output benchmarks/demo.gif

Set FontSize 18
Set Width 1100
Set Height 640
Set Theme "Dracula"
Set Padding 20

Type "rm -rf .argus/demo-workspace"
Enter
Sleep 500ms

Type "# An AI agent says 'done' — Argus makes it prove it."
Enter
Sleep 1s

Type "target/debug/argus demo --workspace .argus/demo-workspace"
Enter
Sleep 3s

Type "target/debug/argus trace show .argus/demo-workspace/.argus/demo.jsonl"
Enter
Sleep 4s
