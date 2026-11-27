#!/bin/bash
set -eu
cd "$(git rev-parse --show-toplevel)"
app=${IOS_CHECKOUT:-../scoracle-ios-classified-headlines}/Scoracle
build=$(mktemp -d)
trap 'rm -f "$build/pilot"; rmdir "$build"' EXIT
swiftc -target arm64-apple-macos26.4 -enable-bare-slash-regex \
  scripts/classified-headlines/Pilot.swift "$app"/Networking/*.swift \
  "$app"/Harness/{ConversationContext,Workspace}.swift "$app"/Models/{Models,LocalStore}.swift \
  "$app"/Skills/*.swift "$app"/Inference/AppleFoundationLanguageModel.swift -o "$build/pilot"
"$build/pilot" "$@"
