Qurb for Windows -- built, NOT tested on Windows
===============================================

Read this first. These programs were cross-compiled on Linux. Nobody has run
them on a Windows computer yet. Qurb is finished for Linux and Android; Windows
comes after, and this build is the first step towards it, not a release of it.

What is here

  qurb-desktop.exe   The window: set up this computer, see your files and
                     devices, pair a phone. Needs Microsoft Edge WebView2,
                     which Windows 10 and 11 normally already have.
  qurb.exe           The same engine from a terminal (Command Prompt or
                     PowerShell): qurb help lists what it does.
  WebView2Loader.dll What the window finds WebView2 with; keep it beside
                     qurb-desktop.exe. From Microsoft's WebView2 SDK.

What is known not to work, or not to be checked

  - None of it has been watched working on Windows. Expect faults; please
    report what you see.
  - The computer is not kept awake while a phone collects a large file (on
    Linux it is).
  - Opening a large video from somebody's folder opens it in your browser,
    which plays some formats and not others (on Linux, in your video player).
  - Starting at sign-in uses your user's Run entry in the registry; turn it
    off in Qurb's Settings or in Task Manager, Startup apps.
  - Not signed: Windows SmartScreen will warn that the publisher is unknown.

Where Qurb keeps things

  Your files:              %USERPROFILE%\qurb
  Its key and its index:   the hidden .qurb folder inside the synced folder
  Which folders it runs:   %APPDATA%\qurb

More: https://github.com/saqibsiddiq/Qurb -- docs/releasing.md
