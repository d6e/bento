class Bento < Formula
  desc "Fast sprite atlas packer with automatic trimming and multiple packing heuristics"
  homepage "https://github.com/d6e/bento"
  url "https://github.com/d6e/bento/archive/refs/tags/v0.4.0.tar.gz"
  sha256 "94eb0eb80af964ada591213d05133d52ffd871a542a252f62f234fcab54451d0"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--features", "gui", *std_cargo_args
  end

  test do
    system "#{bin}/bento", "--version"
  end
end
