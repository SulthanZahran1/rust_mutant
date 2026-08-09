class RustMutant < Formula
  desc "AST-based mutation testing for Rust"
  homepage "https://github.com/SulthanZahran1/rust_mutant"
  version "1.0.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/SulthanZahran1/rust_mutant/releases/download/v1.0.0/rust-mutant-aarch64-apple-darwin.tar.gz"
    else
      url "https://github.com/SulthanZahran1/rust_mutant/releases/download/v1.0.0/rust-mutant-x86_64-apple-darwin.tar.gz"
    end
  end
  on_linux do
    url "https://github.com/SulthanZahran1/rust_mutant/releases/download/v1.0.0/rust-mutant-x86_64-unknown-linux-gnu.tar.gz"
  end

  def install
    binary = Dir["*/rust-mutant"].first || "rust-mutant"
    bin.install binary => "rust-mutant"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/rust-mutant --version")
  end
end
