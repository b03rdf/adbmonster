# ADB Monster VPN Helper

This Android helper uses `VpnService` and HEV tun2socks to route only the selected
application through the desktop-side ADB Monster SOCKS5 relay. It does not need
root access and does not decrypt application traffic.

Build the debug APK with:

```powershell
gradle -p vpn-helper :app:testDebugUnitTest :app:assembleDebug
```

The desktop build embeds `src-tauri/resources/adb-monster-vpn-helper.apk`. After
changing the helper, rebuild it and replace that file before compiling Tauri.

Version 1.0.1 declares every method registered by HEV's JNI loader. The JVM
contract test checks those method signatures without initializing the Android
service. QUERY_ALL_PACKAGES lets this internal testing helper resolve arbitrary
user-selected application packages on Android 11 and later.

Version 1.0.2 extends the device-side emergency lifetime limit to 3690 seconds.
The desktop adds a 90-second startup/recovery allowance to the configured
10–3600-second scenario, starts its scenario clock only after VPN confirmation,
and stops the VPN at normal completion. If the desktop crashes, device expiry
can include that additional allowance. Phase scheduling and measurements remain
on the desktop; see [scenario semantics and acceptance checks](../docs/weak-network-scenarios.md).

Version 1.0.3 explicitly releases the native tunnel and VPN file descriptor before
stopping the service, on manual stop, expiry, revocation and startup failure.
Android's VPN binding can keep a service alive after stopService()/stopSelf(), so
cleanup cannot depend on onDestroy(). The status flag is updated only after
release, and desktop confirmation also checks that the helper's VPN addresses
have disappeared. Upgrade older helpers before testing network recovery.

After a successful build, synchronize the embedded APK from the repository root:

```powershell
Copy-Item vpn-helper/app/build/outputs/apk/debug/app-debug.apk src-tauri/resources/adb-monster-vpn-helper.apk -Force
```

The native `libhev-socks5-tunnel.so` binaries are built from the MIT-licensed
HEV Socks5 Tunnel source pinned in `third_party/NOTICE.txt`.
