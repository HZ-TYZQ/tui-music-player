Name:           music-player
Version:        0.1.0
Release:        1%{?dist}
Summary:        Terminal music player with an FFmpeg playback backend

License:        MIT
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  /usr/bin/ffplay
Requires:       /usr/bin/ffplay

ExclusiveArch:  x86_64

%description
Music Player scans a directory for audio files and provides a terminal user
interface for browsing and playback. It uses the system audio utility supplied
by FFmpeg as its playback backend.

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
* Fri Aug 14 2026 HZ-TYZQ - 0.1.0-1
- Initial Fedora 44 RPM package
