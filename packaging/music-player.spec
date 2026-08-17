Name:           music-player
Version:        1.1.0
Release:        1%{?dist}
Summary:        Terminal music library player

License:        MIT AND Apache-2.0 AND MPL-2.0 AND BSD-3-Clause
URL:            https://github.com/HZ-TYZQ/tui-music-player
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  desktop-file-utils
BuildRequires:  pkgconfig(alsa)
BuildRequires:  pkgconfig(sqlite3)

ExclusiveArch:  x86_64

%description
Music Player provides a responsive terminal interface for a local music
library. It uses Rodio for playback, Lofty for metadata, SQLite for an
incremental index that can be rebuilt, and XDG directories for settings
and playlists.

%prep
%autosetup -n %{name}-%{version}
tar -xzf %{SOURCE1}

%build
export CARGO_HOME="$PWD/.cargo-home"
export CARGO_TARGET_DIR="$PWD/.cargo-target"
mkdir -p "$CARGO_HOME"
cp vendor-config.toml "$CARGO_HOME/config.toml"
cargo build --release --locked --offline

%check
export CARGO_HOME="$PWD/.cargo-home"
export CARGO_TARGET_DIR="$PWD/.cargo-target"
cargo test --release --locked --offline
desktop-file-validate packaging/%{name}.desktop

%install
install -Dpm 0755 .cargo-target/release/%{name} %{buildroot}%{_bindir}/%{name}
install -Dpm 0644 packaging/%{name}.1 %{buildroot}%{_mandir}/man1/%{name}.1
install -Dpm 0644 packaging/%{name}.desktop %{buildroot}%{_datadir}/applications/%{name}.desktop
install -Dpm 0644 assets/icons/%{name}.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{name}.svg
install -Dpm 0644 assets/icons/%{name}-48.png %{buildroot}%{_datadir}/icons/hicolor/48x48/apps/%{name}.png

%files
%license LICENSE packaging/licenses/Apache-2.0.txt packaging/licenses/MPL-2.0.txt
%doc README.md changelog.md packaging/licenses/THIRD-PARTY-NOTICES.txt
%{_bindir}/%{name}
%{_mandir}/man1/%{name}.1*
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{name}.svg
%{_datadir}/icons/hicolor/48x48/apps/%{name}.png

%changelog
* Mon Aug 17 2026 HZ-TYZQ - 1.1.0-1
- Replace GStreamer playback and Discoverer with Rodio and Lofty
- Rebuild the spectrum from a PCM tap and rustfft
- Build against ALSA; let rpmbuild generate runtime ELF dependencies
- Ship Apache-2.0, MPL-2.0, and third-party notices

* Sat Aug 15 2026 HZ-TYZQ - 1.0.2-1
- Add Windows 11 x86_64 MSVC builds, tests, and release packaging
- Bundle a private GStreamer runtime and SQLite in Windows packages
- Keep Fedora linked to its system GStreamer and SQLite libraries

* Sat Aug 15 2026 HZ-TYZQ - 1.0.1-1
- Add desktop integration and a restrained monochrome terminal theme
- Preserve the selected track after background rescans with active fuzzy search
- Try the next track after asynchronous playback errors

* Fri Aug 14 2026 HZ-TYZQ - 1.0.0-1
- Publish the first stable release with playback, library management, playlists, search, and audio visualization
- Keep asynchronous Nucleo search results synchronized with matcher snapshots

* Fri Aug 14 2026 HZ-TYZQ - 0.3.0-3
- Reduce low-frequency visualizer saturation with interpolated sampling and display headroom

* Fri Aug 14 2026 HZ-TYZQ - 0.3.0-2
- Refine the spectrum to 50-8000 Hz logarithmic bars and action-oriented playback icons

* Fri Aug 14 2026 HZ-TYZQ - 0.3.0-1
- Add an integrated GStreamer spectrum visualizer with a persistent toggle

* Fri Aug 14 2026 HZ-TYZQ - 0.2.0-1
- Add GStreamer playback, background library index, search, queue, and playlists

* Fri Aug 14 2026 HZ-TYZQ - 0.1.0-1
- Initial Fedora 44 RPM package
