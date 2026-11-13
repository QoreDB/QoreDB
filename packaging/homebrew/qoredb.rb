# SPDX-License-Identifier: Apache-2.0
#
# Homebrew cask for QoreDB — the open-source modern database client.
#
# This file is the canonical source. To publish, copy it into
# homebrew-cask (homebrew/homebrew-cask) under Casks/q/qoredb.rb and
# open a PR. Update `version` and the arm/intel `sha256` values for each new release.
#
# Generating sha256 values:
#   shasum -a 256 QoreDB_<version>_x64.dmg
#   shasum -a 256 QoreDB_<version>_aarch64.dmg

cask "qoredb" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.41"
  sha256 arm:   "ad989ba6a695c225bbcb5f223ade332aae7996786ec27179bb572e3e55f9ee1c",
         intel: "df612cb3d8db725d8246222cc958b09ae0156a1bacb97ac4704d59b51a8df49a"

  url "https://github.com/QoreDB/QoreDB/releases/download/v#{version}/QoreDB_#{version}_#{arch}.dmg",
      verified: "github.com/QoreDB/QoreDB/"
  name "QoreDB"
  desc "Database client for SQL and NoSQL databases"
  homepage "https://www.qoredb.com/"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true

  app "QoreDB.app"

  zap trash: [
    "~/Library/Application Support/com.rapha.qoredb",
    "~/Library/Caches/com.rapha.qoredb",
    "~/Library/Preferences/com.rapha.qoredb.plist",
    "~/Library/Saved Application State/com.rapha.qoredb.savedState",
    "~/Library/WebKit/com.rapha.qoredb",
  ]
end
