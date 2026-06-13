# Argus demo — record with VHS (https://github.com/charmbracelet/vhs):
#   vhs benchmarks/demo.tape   ->   produces demo.gif
#
# Requires: argus on PATH, ANTHROPIC_API_KEY set, python3 available.
# This records the money shot: the agent claims done, the verification gate
# catches the failing test, and the agent fixes it for real.

Output demo.gif

Set FontSize 18
Set Width 1100
Set Height 640
Set Theme "Dracula"
Set Padding 20

Type "cd benchmarks/cases/fix-fibonacci"
Enter
Sleep 1s

Type "# An AI agent says 'done' — Argus makes it prove it."
Enter
Sleep 1500ms

Type "argus run 'fix the bug in fib.py' --provider anthropic --model claude-sonnet-4-5 --yes --verify 'python3 test_fib.py'"
Enter
Sleep 12s

Type "python3 test_fib.py   # verified for real"
Enter
Sleep 2s

Type "argus tui   # browse the black box of what just happened"
Enter
Sleep 3s
