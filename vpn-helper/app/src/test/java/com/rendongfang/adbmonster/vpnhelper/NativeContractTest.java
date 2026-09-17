package com.rendongfang.adbmonster.vpnhelper;

import org.junit.Test;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import static org.junit.Assert.*;

public final class NativeContractTest {
    @Test
    public void declaresEveryMethodRegisteredByHev() throws Exception {
        // Do not initialize the Android service or load an Android ELF on the host JVM.
        Class<?> service = Class.forName("hev.sockstun.TProxyService", false,
                getClass().getClassLoader());
        assertNative(service, "TProxyStartService", boolean.class, String.class, int.class);
        assertNative(service, "TProxyStopService", boolean.class);
        assertNative(service, "TProxyIsRunning", boolean.class);
        assertNative(service, "TProxyGetStats", long[].class);
    }

    private void assertNative(Class<?> service, String name, Class<?> result,
                              Class<?>... parameters) throws Exception {
        Method method = service.getDeclaredMethod(name, parameters);
        assertTrue(Modifier.isNative(method.getModifiers()));
        assertEquals(result, method.getReturnType());
    }
}
