Name:           music-player
Version:        0.2.0
Release:        1%{?dist}
Summary:        Terminal music library player powered by GStreamer

License:        MIT
URL:            https://github.com/HZ-TYZQ/tui-music-player
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconfig(gstreamer-1.0)
BuildRequires:  pkgconfig(gstreamer-audio-1.0)
BuildRequires:  pkgconfig(gstreamer-play-1.0)
BuildRequires:  pkgconfig(gstreamer-pbutils-1.0)
BuildRequires:  pkgconfig(sqlite3)
Requires:       gstreamer1
Requires:       gstreamer1-plugins-base
Requires:       gstreamer1-plugins-good
Requires:       gstreamer1-plugins-bad-free
Requires:       gstreamer1-plugins-ugly-free
Requires:       gstreamer1-plugin-libav

ExclusiveArch:  x86_64

%description
Music Player provides a responsive terminal interface for a local music
library. It uses GStreamer for metadata and playback, SQLite for an incremental
index that can be rebuilt, and XDG directories for settings and playlists.

%prep
%autosetup -n %{name}-%{version}
tar -xzf %{SOURCE1}

%build
export CARGO_HOME="$PWD/.cargo-home"
mkdir -p "$CARGO_HOME"
cp vendor-config.toml "$CARGO_HOME/config.toml"
cargo build --release --locked --offline

%check
export CARGO_HOME="$PWD/.cargo-home"
cargo test --release --locked --offline

%install
install -Dpm 0755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
install -Dpm 0644 packaging/%{name}.1 %{buildroot}%{_mandir}/man1/%{name}.1

%files
%license LICENSE
%doc README.md changelog.md
%{_bindir}/%{name}
%{_mandir}/man1/%{name}.1*

%changelog
* Fri Aug 14 2026 HZ-TYZQ - 0.2.0-1
- Add GStreamer playback, background library index, search, queue, and playlists

* Fri Aug 14 2026 HZ-TYZQ - 0.1.0-1
- Initial Fedora 44 RPM package
