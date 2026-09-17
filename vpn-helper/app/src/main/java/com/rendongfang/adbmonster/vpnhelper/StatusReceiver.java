package com.rendongfang.adbmonster.vpnhelper;

import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.VpnService;

import hev.sockstun.TProxyService;

public final class StatusReceiver extends BroadcastReceiver {
    public static final String ACTION_STATUS =
            "com.rendongfang.adbmonster.vpnhelper.STATUS";
    public static final String VERSION_NAME = "1.0.3";

    @Override
    public void onReceive(Context context, Intent intent) {
        if (!ACTION_STATUS.equals(intent.getAction())) {
            return;
        }

        SharedPreferences prefs = context.getSharedPreferences(
                TProxyService.PREFS_NAME, Context.MODE_PRIVATE);
        boolean running = TProxyService.isRunning();
        boolean authorized = VpnService.prepare(context) == null;
        String target = running ? prefs.getString(TProxyService.PREF_TARGET, "") : "";
        long expiresAt = running ? prefs.getLong(TProxyService.PREF_EXPIRES_AT, 0L) : 0L;
        setResultCode(Activity.RESULT_OK);
        setResultData((running ? "1" : "0") + "|"
                + (authorized ? "1" : "0") + "|"
                + VERSION_NAME + "|" + target + "|" + expiresAt);
    }
}
