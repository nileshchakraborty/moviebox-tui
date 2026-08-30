class MovieboxTui < Formula
  VERSION = "0.1.13"
  MACOS_SHA256 = "1ed1212606244ba1a7444ede704f2c4fac8b8639c529c4938c834b2448cce079"
  LINUX_X64_SHA256 = "61d306474bf60e16c3503120d883d0a940b6543ebfc2c9445bf9da5898a736bc"
  LINUX_ARM64_SHA256 = "d58f88a0a6e5cb34e8150fb0a01776ba16ce0d238a46bd85f3865d728fb2e0bb"

  desc "Stream movies, shows, anime, and live TV from your terminal"
  homepage "https://github.com/nileshchakraborty/moviebox-tui"
  version VERSION
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    url "https://github.com/nileshchakraborty/moviebox-tui/releases/download/v#{VERSION}/MovieBox_macOS_Universal.tar.gz"
    sha256 MACOS_SHA256
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/nileshchakraborty/moviebox-tui/releases/download/v#{VERSION}/MovieBox_Linux_arm64.tar.gz"
      sha256 LINUX_ARM64_SHA256
    else
      url "https://github.com/nileshchakraborty/moviebox-tui/releases/download/v#{VERSION}/MovieBox_Linux_x64.tar.gz"
      sha256 LINUX_X64_SHA256
    end
  end

  def install
    bin.install "moviebox-tui"
  end

  test do
    system "#{bin}/moviebox-tui", "--version"
  end
end
