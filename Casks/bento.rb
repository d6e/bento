cask "bento" do
  version "0.4.0"
  sha256 "47745421562127bd8a22f1e45c39fb057292537bac6a3b995698e0a5531da50e"

  # TODO: Update to universal DMG in next release (v0.5.0+)
  # url "https://github.com/d6e/bento/releases/download/v#{version}/Bento_#{version}_universal.dmg"
  url "https://github.com/d6e/bento/releases/download/v#{version}/Bento_#{version}_aarch64.dmg"
  name "Bento"
  desc "Fast sprite atlas packer with automatic trimming and multiple packing heuristics"
  homepage "https://github.com/d6e/bento"

  # TODO: Remove arch restriction once universal DMG is available
  depends_on arch: :arm64

  app "Bento.app"
end
