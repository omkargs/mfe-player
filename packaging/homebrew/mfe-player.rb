# Homebrew formula template for a future omkargs/homebrew-mfe tap.
#
#   brew tap omkargs/mfe
#   brew install mfe-player
#
# Replace the sha256 after tagging a release:
#   curl -sL https://github.com/omkargs/mfe-player/archive/refs/tags/v0.2.1.tar.gz | shasum -a 256
class MfePlayer < Formula
  desc "Plugin-driven terminal music player with mpv, no subscription"
  homepage "https://github.com/omkargs/mfe-player"
  url "https://github.com/omkargs/mfe-player/archive/refs/tags/v0.2.1.tar.gz"
  sha256 "REPLACE_WITH_RELEASE_SHA256"
  license "MIT"

  depends_on "rust" => :build
  depends_on "mpv"
  depends_on "yt-dlp"
  depends_on "python@3.12"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "mfe-player", shell_output("#{bin}/mfe-player --version")
    assert_match "mfe-player doctor", shell_output("#{bin}/mfe-player --help")
  end
end
