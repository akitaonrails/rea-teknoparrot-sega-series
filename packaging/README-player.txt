Sega Racing Arcade Series Launcher
==================================

One launcher for seven Sega arcade racers: OutRun 2 SP SDX, OutRun 2 SP
Special Tours, Crazy Taxi High Roller, SEGA Rally 3, SEGA Racing Classic,
Daytona Championship USA and F355 Challenge 2. It contains no game files: you need your own
TeknoParrot dumps, one folder each.

Start
-----

Linux: run ./sega-launcher from this folder.
Windows: run sega-launcher.exe.

The first window asks for the folder that holds your game folders. Add it,
and the launcher lists what it found. For F355 Challenge 2 and the two
Chihiro games, add your BIOS folder (EmuDeck's bios folder works) under
Settings, Folders and BIOS.

What works today
----------------

- SEGA Rally 3, SEGA Racing Classic and Daytona Championship USA: full races
  (Linux; Windows build runs without Wine).
- OutRun 2 SP SDX: full races (Linux only).
- OutRun 2 SP Special Tours and Crazy Taxi High Roller: boot and render in
  xemu (Chihiro fork, Linux and Windows); requires Chihiro BIOS files
  (chihiro_xbox_bios.bin, fpr21042_m29w160et.bin, ic10/ic11/pc20 EEPROMs,
  and ver1305.bin for OutRun 2 SP).
- F355 Challenge 2: boots and renders up to 4K (4:3) in Flycast (Linux and
  Windows); requires f355bios.zip. The first start can take a minute or two.

Controls
--------

A wheel or a pad. Pad: triggers are the pedals, the left stick steers,
the shoulder buttons shift, Y changes the view, B is the handbrake, Start
starts, Back inserts a coin. Each game's steering lock and force feedback
strength are in Settings. A pad also works the launcher's window: d-pad to
move, A to choose, B to go back, Start to play, Y for Settings.

Where things live
-----------------

Linux: ~/.local/share/rea-sega/ (settings, each game's runtime with its
rankings and log, the Wine prefix). Windows: %LOCALAPPDATA%\rea-sega\.
To uninstall, delete this folder and that one (copy runtimes/ first to keep
your rankings).

More: https://github.com/akitaonrails/rea-teknoparrot-sega-series
Licences and sources: the licenses/ folder.
