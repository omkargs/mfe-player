# Record a README demo with vhs (https://github.com/charmbracelet/vhs):
#   vhs docs/demo.tape
# Requires: cargo build --release, fonts with a Nerd Font or use icons = "ascii".
#
# Tune the typing speed down if the GIF feels slow (the $SleepCommand values
# are what give the cava panel time to react to the music).

Output docs/demo.gif
Set FontSize 20
Set Width 1400
Set Height 800
Set Shell zsh

Type "mfe daft punk"
Enter
Sleep 3s
Type@200ms "/"
Sleep 500ms
Type "get lucky"
Sleep 2s
Enter
Sleep 10s
# let the spectrum dance for a bit
Sleep 5s
Alt+v
Sleep 4s
Alt+t
Sleep 1s
Alt+t
Sleep 2s
Ctrl+b
Sleep 3s
Type "mfe status --json"
Enter
Sleep 2s
Type "mfe"
Enter
Sleep 3s
