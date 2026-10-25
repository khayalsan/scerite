package dev.khayal.scerite.velocity;

import com.google.inject.Inject;
import com.velocitypowered.api.event.Continuation;
import com.velocitypowered.api.event.Subscribe;
import com.velocitypowered.api.event.player.PlayerChooseInitialServerEvent;
import com.velocitypowered.api.event.player.ServerPreConnectEvent;
import com.velocitypowered.api.event.proxy.ProxyInitializeEvent;
import com.velocitypowered.api.event.proxy.ProxyPingEvent;
import com.velocitypowered.api.plugin.Plugin;
import com.velocitypowered.api.plugin.annotation.DataDirectory;
import com.velocitypowered.api.proxy.ProxyServer;
import com.velocitypowered.api.proxy.server.RegisteredServer;
import com.velocitypowered.api.proxy.server.ServerInfo;
import com.velocitypowered.api.util.Favicon;
import java.io.IOException;
import java.io.InputStream;
import java.io.Reader;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Properties;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer;
import org.slf4j.Logger;

@Plugin(id = "sceritevelocity", name = "Scerite Velocity", version = "1.1.1")
public final class SceriteVelocityPlugin {
    private static final String DEFAULT_MOTD = "A Minecraft Server";
    private final ProxyServer proxy;
    private final Logger logger;
    private final Path dataDirectory;
    private final Object startLock = new Object();
    private final AtomicInteger pendingJoins = new AtomicInteger();
    private volatile CompletableFuture<Boolean> starting;
    private volatile long emptySince = -1;
    private volatile boolean stopping;
    private RegisteredServer backend;
    private Path startScript;
    private Path stopScript;
    private int backendPort;
    private int idleSeconds;
    private Path settingsFile;
    private Path serverDirectory;
    private volatile Component serverDescription = Component.text(DEFAULT_MOTD);
    private volatile Favicon serverFavicon;

    @Inject
    public SceriteVelocityPlugin(ProxyServer proxy, Logger logger, @DataDirectory Path dataDirectory) {
        this.proxy = proxy;
        this.logger = logger;
        this.dataDirectory = dataDirectory;
    }

    @Subscribe
    public void initialize(ProxyInitializeEvent event) {
        Properties settings = new Properties();
        settingsFile = dataDirectory.resolve("config.properties");
        try (InputStream input = Files.newInputStream(settingsFile)) {
            settings.load(input);
            String name = settings.getProperty("serverName");
            backendPort = Integer.parseInt(settings.getProperty("backendPort"));
            idleSeconds = Integer.parseInt(settings.getProperty("idleSeconds", "300"));
            startScript = Path.of(settings.getProperty("startScript")).toAbsolutePath();
            stopScript = Path.of(settings.getProperty("stopScript")).toAbsolutePath();
            String directory = settings.getProperty("serverDirectory");
            if (directory != null && !directory.isBlank()) serverDirectory = Path.of(directory).toAbsolutePath();
            if (name == null || name.isBlank() || backendPort < 1 || backendPort > 65535 || idleSeconds < 30) {
                throw new IllegalArgumentException("Invalid server name, port, or idle delay");
            }
            backend = proxy.registerServer(new ServerInfo(name, new InetSocketAddress("127.0.0.1", backendPort)));
        } catch (Exception error) {
            logger.error("Scerite Velocity configuration is invalid", error);
            return;
        }
        refreshServerDetails();
        proxy.getScheduler().buildTask(this, this::checkIdle).repeat(10, TimeUnit.SECONDS).schedule();
        logger.info("Scerite on-demand backend ready");
    }

    @Subscribe
    public void proxyPing(ProxyPingEvent event) {
        Component description = serverDescription;
        Favicon favicon = serverFavicon;
        if (serverDirectory == null) return;
        var builder = event.getPing().asBuilder();
        builder.description(description);
        if (favicon != null) builder.favicon(favicon);
        else builder.clearFavicon();
        event.setPing(builder.build());
    }

    private void refreshServerDetails() {
        if (serverDirectory == null) return;
        try {
            Path propertiesPath = serverDirectory.resolve("server.properties");
            if (Files.isRegularFile(propertiesPath) && Files.size(propertiesPath) <= 1024 * 1024) {
                Properties properties = new Properties();
                try (Reader input = Files.newBufferedReader(propertiesPath, StandardCharsets.UTF_8)) { properties.load(input); }
                String motd = properties.getProperty("motd", DEFAULT_MOTD);
                serverDescription = LegacyComponentSerializer.legacySection().deserialize(motd);
            } else {
                serverDescription = Component.text(DEFAULT_MOTD);
            }
        } catch (Exception error) {
            logger.warn("Could not read the backend MOTD", error);
        }
        try {
            Path iconPath = serverDirectory.resolve("server-icon.png");
            if (Files.isRegularFile(iconPath) && Files.size(iconPath) <= 1024 * 1024) {
                serverFavicon = Favicon.create(iconPath);
            } else {
                serverFavicon = null;
            }
        } catch (Exception error) {
            logger.warn("Could not read the backend server icon", error);
        }
    }

    private void refreshIdleDelay() {
        try (InputStream input = Files.newInputStream(settingsFile)) {
            Properties settings = new Properties();
            settings.load(input);
            int updated = Integer.parseInt(settings.getProperty("idleSeconds", "300"));
            if (updated >= 60 && updated <= 7200) idleSeconds = updated;
        } catch (Exception error) {
            logger.warn("Could not read the idle delay", error);
        }
    }

    @Subscribe
    public void chooseInitialServer(PlayerChooseInitialServerEvent event) {
        if (backend != null) event.setInitialServer(backend);
    }

    @Subscribe
    public void beforeServerConnect(ServerPreConnectEvent event, Continuation continuation) {
        if (backend == null || !event.getOriginalServer().getServerInfo().getName()
                .equals(backend.getServerInfo().getName())) {
            continuation.resume();
            return;
        }
        pendingJoins.incrementAndGet();
        emptySince = -1;
        ensureStarted().whenComplete((ready, error) -> {
            if (error != null || !Boolean.TRUE.equals(ready)) {
                event.setResult(ServerPreConnectEvent.ServerResult.denied());
                logger.error("Could not start backend for {}", event.getPlayer().getUsername(), error);
            }
            pendingJoins.decrementAndGet();
            continuation.resume();
        });
    }

    private CompletableFuture<Boolean> ensureStarted() {
        if (isListening()) return CompletableFuture.completedFuture(true);
        synchronized (startLock) {
            if (isListening()) return CompletableFuture.completedFuture(true);
            if (starting != null && !starting.isDone()) return starting;
            starting = CompletableFuture.supplyAsync(() -> {
                try {
                    runScript(startScript);
                    for (int attempt = 0; attempt < 120; attempt++) {
                        if (isListening()) return true;
                        Thread.sleep(1000);
                    }
                    logger.error("Backend did not become ready within 120 seconds");
                } catch (Exception error) {
                    logger.error("Could not start backend", error);
                }
                return false;
            });
            return starting;
        }
    }

    private void checkIdle() {
        refreshIdleDelay();
        refreshServerDetails();
        if (backend == null || pendingJoins.get() > 0 || !backend.getPlayersConnected().isEmpty()) {
            emptySince = -1;
            return;
        }
        if (!isListening()) {
            emptySince = -1;
            stopping = false;
            return;
        }
        long now = System.currentTimeMillis();
        if (emptySince < 0) emptySince = now;
        if (stopping || now - emptySince < idleSeconds * 1000L) return;
        stopping = true;
        CompletableFuture.runAsync(() -> {
            try {
                runScript(stopScript);
            } catch (Exception error) {
                logger.error("Could not stop idle backend", error);
            } finally {
                emptySince = System.currentTimeMillis();
                stopping = false;
            }
        });
    }

    private boolean isListening() {
        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress("127.0.0.1", backendPort), 500);
            return true;
        } catch (IOException error) {
            return false;
        }
    }

    private void runScript(Path script) throws IOException, InterruptedException {
        Process process = new ProcessBuilder("/bin/sh", script.toString())
                .redirectErrorStream(true).start();
        if (!process.waitFor(15, TimeUnit.SECONDS) || process.exitValue() != 0) {
            process.destroyForcibly();
            throw new IOException("Script failed: " + script.getFileName());
        }
    }
}
