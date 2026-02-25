%global crate_version %{?version}%{!?version:0.0.0}

Name:           bankero
Version:        %{crate_version}
Release:        1%{?dist}
Summary:        Local-first multi-currency ledger (CLI)
License:        UNLICENSED
URL:            https://github.com/JoCarrasco/bankero
Source0:        %{name}-%{crate_version}.tar.gz

BuildArch:      x86_64

%description
Bankero is a local-first, multi-currency ledger optimized for CLI workflows.

%prep
%setup -q

%build
# No build here: the binary is provided prebuilt in Source0.

%install
rm -rf %{buildroot}
install -D -m 0755 bankero %{buildroot}%{_bindir}/bankero
install -D -m 0644 README.md %{buildroot}%{_docdir}/%{name}/README.md
install -D -m 0644 PRD.md %{buildroot}%{_docdir}/%{name}/PRD.md

%files
%{_bindir}/bankero
%doc %{_docdir}/%{name}/README.md
%doc %{_docdir}/%{name}/PRD.md

%changelog
* Tue Feb 25 2026 Bankero Maintainers <noreply@github.com> - %{crate_version}-1
- Automated build
