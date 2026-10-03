# Bader for iPhone

A Bader of its own: it talks to the AI service, Gmail and Google Calendar directly, so it works when the
computer is off. Memory is shared with the computer through one file in the user's Google Drive.

    brew install xcodegen
    cd bader/ios && xcodegen generate
    xcodebuild test -project Bader.xcodeproj -scheme Bader -destination 'platform=iOS Simulator,name=iPhone 17 Pro' CODE_SIGNING_ALLOWED=NO
    open Bader.xcodeproj        # pick your team, run on a phone

Pairing: on the computer open Settings → "Bader on iPhone" → Show pairing code, then scan it in the app.
No keys live in this repository; the phone receives them from the code and keeps them in its Keychain.

| File | What it does |
|---|---|
| `Model/Brain.swift` | the model + tools (mail, meetings, news, weather, recall, remember, send mail, create meeting) |
| `Model/Google.swift` | Gmail, Calendar, the memory file in Drive |
| `Model/Memory.swift` | shared memory: same line format and merge as `bader_sync.py` |
| `Model/Voice.swift` | recording, speech to text (OpenRouter), reading aloud (built-in voices) |
| `Model/FaceLink.swift`, `FaceScreen.swift` | Bluetooth link to the small display and what is drawn on it |
| `Views/` | pairing, chat, settings |
