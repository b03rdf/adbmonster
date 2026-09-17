package hev.sockstun;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.net.VpnService;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.ParcelFileDescriptor;
import android.util.Log;

import com.rendongfang.adbmonster.vpnhelper.R;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

public final class TProxyService extends VpnService {
    public static final String ACTION_START =
            "com.rendongfang.adbmonster.vpnhelper.START_SERVICE";
    public static final String ACTION_STOP =
            "com.rendongfang.adbmonster.vpnhelper.STOP_SERVICE";
    public static final String PREFS_NAME = "weak_network_status";
    public static final String PREF_TARGET = "target_package";
    public static final String PREF_EXPIRES_AT = "expires_at";

    private static final String CHANNEL_ID = "weak-network";
    private static final int NOTIFICATION_ID = 73;
    private static volatile boolean running;

    private static native boolean TProxyStartService(String configPath, int fd);
    private static native boolean TProxyStopService();
    private static native boolean TProxyIsRunning();
    // HEV registers this method during JNI_OnLoad, even when stats are unused.
    private static native long[] TProxyGetStats();

    static {
        System.loadLibrary("hev-socks5-tunnel");
    }

    private final Handler handler = new Handler(Looper.getMainLooper());
    private ParcelFileDescriptor tunFd;
    private Runnable expiryTask;

    public static boolean isRunning() {
        // A stopped flag alone must never hide a live native tunnel.
        return running || TProxyIsRunning();
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && ACTION_STOP.equals(intent.getAction())) {
            stopAndRelease();
            return START_NOT_STICKY;
        }
        if (intent == null || !ACTION_START.equals(intent.getAction())) {
            stopAndRelease();
            return START_NOT_STICKY;
        }

        String targetPackage = intent.getStringExtra("target_package");
        createNotification(targetPackage == null ? "" : targetPackage);
        startTunnel(intent);
        return START_NOT_STICKY;
    }

    @Override
    public void onRevoke() {
        stopAndRelease();
        super.onRevoke();
    }

    @Override
    public void onDestroy() {
        stopTunnel(true);
        super.onDestroy();
    }

    private synchronized void startTunnel(Intent intent) {
        stopTunnel(false);

        String targetPackage = intent.getStringExtra("target_package");
        String username = safeToken(intent.getStringExtra("proxy_username"));
        String password = safeToken(intent.getStringExtra("proxy_password"));
        int proxyPort = intent.getIntExtra("proxy_port", 0);
        int durationSeconds = intent.getIntExtra("duration_seconds", 0);

        if (!validPackageName(targetPackage)
                || username.isEmpty()
                || password.isEmpty()
                || proxyPort < 1 || proxyPort > 65535
                || durationSeconds < 10 || durationSeconds > 3690) {
            stopAndRelease();
            return;
        }

        try {
            getPackageManager().getPackageInfo(targetPackage, 0);
        } catch (PackageManager.NameNotFoundException error) {
            stopAndRelease();
            return;
        }

        Builder builder = new Builder()
                .setSession("ADB Monster 弱网 - " + targetPackage)
                .setMtu(1500)
                .setBlocking(false)
                .addAddress("10.111.222.1", 30)
                .addRoute("0.0.0.0", 0)
                .addAddress("fd00:111:222::1", 126)
                .addRoute("::", 0)
                .addDnsServer("198.18.0.2");
        try {
            builder.addAllowedApplication(targetPackage);
        } catch (PackageManager.NameNotFoundException error) {
            stopAndRelease();
            return;
        }

        tunFd = builder.establish();
        if (tunFd == null) {
            stopAndRelease();
            return;
        }

        // The VPN exists as soon as establish() returns, before native startup.
        running = true;

        File configFile = new File(getCacheDir(), "tun2socks.yml");
        String config = "misc:\n"
                + "  task-stack-size: 24576\n"
                + "tunnel:\n"
                + "  mtu: 1500\n"
                + "  icmp: 'reply'\n"
                + "socks5:\n"
                + "  address: '127.0.0.1'\n"
                + "  port: " + proxyPort + "\n"
                + "  udp: 'tcp'\n"
                + "  username: '" + username + "'\n"
                + "  password: '" + password + "'\n"
                + "mapdns:\n"
                + "  address: 198.18.0.2\n"
                + "  port: 53\n"
                + "  network: 240.0.0.0\n"
                + "  netmask: 240.0.0.0\n"
                + "  cache-size: 10000\n";

        try (FileOutputStream output = new FileOutputStream(configFile, false)) {
            output.write(config.getBytes(StandardCharsets.UTF_8));
        } catch (IOException error) {
            stopAndRelease();
            return;
        }

        if (!TProxyStartService(configFile.getAbsolutePath(), tunFd.getFd())) {
            stopAndRelease();
            return;
        }

        long expiresAt = System.currentTimeMillis() + durationSeconds * 1000L;
        getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit()
                .putString(PREF_TARGET, targetPackage)
                .putLong(PREF_EXPIRES_AT, expiresAt)
                .apply();
        running = true;

        expiryTask = this::stopAndRelease;
        handler.postDelayed(expiryTask, durationSeconds * 1000L);
    }

    private void stopAndRelease() {
        // Android binds VpnService while its VPN is active. stopSelf/stopService
        // alone can leave that binding alive and never invoke onDestroy.
        try {
            stopTunnel(true);
        } finally {
            stopSelf();
        }
    }

    private synchronized void stopTunnel(boolean removeNotification) {
        if (expiryTask != null) {
            handler.removeCallbacks(expiryTask);
            expiryTask = null;
        }
        try {
            if (TProxyIsRunning()) {
                TProxyStopService();
            }
        } finally {
            // Always close the VPN descriptor, even if native shutdown fails.
            // Closing the last TUN descriptor removes the VPN interface/routes.
            if (tunFd != null) {
                try {
                    tunFd.close();
                    tunFd = null;
                } catch (IOException error) {
                    Log.e("AdbMonsterVpn", "Failed to close VPN descriptor", error);
                }
            }
            running = tunFd != null || TProxyIsRunning();
            if (!running) {
                getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                        .edit().clear().apply();
            }
            if (removeNotification) {
                stopForeground(Service.STOP_FOREGROUND_REMOVE);
            }
        }
    }

    private void createNotification(String targetPackage) {
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationChannel channel = new NotificationChannel(
                    CHANNEL_ID, getString(R.string.vpn_channel), NotificationManager.IMPORTANCE_LOW);
            manager.createNotificationChannel(channel);
        }

        Notification.Builder builder = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
                ? new Notification.Builder(this, CHANNEL_ID)
                : new Notification.Builder(this);
        Notification notification = builder
                .setSmallIcon(android.R.drawable.stat_sys_warning)
                .setContentTitle(getString(R.string.app_name))
                .setContentText(getString(R.string.vpn_running, targetPackage))
                .setOngoing(true)
                .setCategory(Notification.CATEGORY_SERVICE)
                .build();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
    }

    private static String safeToken(String value) {
        if (value == null) {
            return "";
        }
        return value.matches("[A-Za-z0-9]{1,64}") ? value : "";
    }

    private static boolean validPackageName(String value) {
        return value != null
                && value.length() <= 255
                && value.contains(".")
                && value.matches("[A-Za-z0-9_.]+")
                && !value.equals("com.rendongfang.adbmonster.vpnhelper");
    }
}
