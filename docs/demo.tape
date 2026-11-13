# Record a README demo with vhs (https://github.com/charmbracelet/vhs):
#   vhs docs/demo.tape
# Requires: cargo build --release, fonts with a Nerd Font or use icons = "ascii".
#
# The Sleep values are deliberately generous so the GIF reads slowly in the
# README — the cava panel needs time to visibly react to the music.
# Playback uses a local-library track so the stream resolve never stalls the take.

Output docs/demo.gif
Set FontSize 20
Set Width 1400
Set Height 800
Set Shell zsh
Set Framerate 30
Set TypingSpeed 400ms

Type "mfe daft punk"
Enter
Sleep 5s
# jump to the local library and play - instant, full-length
Type@1ms "8"
Sleep 2s
Enter
Sleep 3s
# let the spectrum dance
Sleep 10s
Sleep 8s
# cycle themes while the spectrum runs
Alt+t
Sleep 5s
Alt+t
Sleep 5s
# more spectrum
Sleep 6s
Ctrl+b
Sleep 5s
Type "mfe status --json"
Enter
Sleep 4s
Type "mfe"
Enter
Sleep 5s
