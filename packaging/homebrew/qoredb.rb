# SPDX-License-Identifier: Apache-2.0
#
# Homebrew cask for QoreDB — the open-source modern database client.
#
# This file is the canonical source. To publish, copy it into
# homebrew-cask (homebrew/homebrew-cask) under Casks/q/qoredb.rb and
# open a PR. Update `version` and the two `sha256` values for each new release.
#
# Generating sha256 values:
#   shasum -a 256 QoreDB_<version>_x64.dmg
#   shasum -a 256 QoreDB_<version>_aarch64.dmg

cask "qoredb" do
  version "0.1.28"

  on_intel do
    sha256 "REPLACE_WITH_X64_DMG_SHA256"
    url "https://github.com/QoreDB/QoreDB/releases/download/v#{version}/QoreDB_#{version}_x64.dmg",
        verified: "github.com/QoreDB/QoreDB/"
  end

  on_arm do
    sha256 "REPLACE_WITH_AARCH64_DMG_SHA256"
    url "https://github.com/QoreDB/QoreDB/releases/download/v#{version}/QoreDB_#{version}_aarch64.dmg",
        verified: "github.com/QoreDB/QoreDB/"
  end

  name "QoreDB"
  desc "Modern, local-first database client for PostgreSQL, MySQL, MongoDB and more"
  homepage "https://www.qoredb.com/"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on macos: ">= :ventura"

  app "QoreDB.app"

  zap trash: [
    "~/Library/Application Support/com.qoredb.app",
    "~/Library/Caches/com.qoredb.app",
    "~/Library/Preferences/com.qoredb.app.plist",
    "~/Library/Saved Application State/com.qoredb.app.savedState",
  ]
end
