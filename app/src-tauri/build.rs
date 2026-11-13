// The release app needs administrator access to configure Wintun, DNS and
// routes. All proxy and MarsLink protocol work remains inside our own process;
// no vendor executable is launched.
//
// Only release builds get the manifest: `tauri dev` is launched by cargo from a
// normal terminal, which cannot satisfy an elevation request.
const ADMIN_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

fn main() {
    if std::path::Path::new("resources/engine/MarsLinkProxy.exe").exists() {
        panic!("vendor MarsLinkProxy.exe must never be bundled");
    }
    let mut windows = tauri_build::WindowsAttributes::new();
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        windows = windows.app_manifest(ADMIN_MANIFEST);
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
