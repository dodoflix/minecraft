import java.net.InetAddress;
import java.net.NetworkInterface;
import java.net.SocketException;
import java.net.UnknownHostException;
import java.net.spi.InetAddressResolver;
import java.net.spi.InetAddressResolverProvider;
import java.util.Arrays;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

/**
 * Name resolution of the e2e client, fail closed: only loopback names, the hosts in -De2e.allow-hosts (comma
 * separated, resolved by the system; the game server's own host or literal address) and the machine's own name
 * (-De2e.local-host, answered with the loopback address) resolve, and an address of a local interface or the game
 * server's literal address has no reverse name. Every other lookup is refused with
 * an UnknownHostException and reported on stdout as "[E2E-NET] {"blocked":"HOST"}", which
 * scripts/mc-client-status.py turns into a failing verdict. Registered through META-INF/services by
 * scripts/real-client.sh. A literal IP address is never looked up, so scripts/e2e/net-watch.py covers connections to
 * those.
 */
public class E2eResolver extends InetAddressResolverProvider {
    @Override
    public InetAddressResolver get(Configuration configuration) {
        InetAddressResolver builtin = configuration.builtinResolver();
        Set<String> allowed = Arrays.stream(System.getProperty("e2e.allow-hosts", "").split(","))
                .map(h -> h.trim().toLowerCase(Locale.ROOT))
                .filter(h -> !h.isEmpty())
                .collect(Collectors.toSet());
        String localHost = System.getProperty("e2e.local-host", "").trim().toLowerCase(Locale.ROOT);
        return new InetAddressResolver() {
            @Override
            public Stream<InetAddress> lookupByName(String host, LookupPolicy policy) throws UnknownHostException {
                String h = host.toLowerCase(Locale.ROOT);
                if (h.equals("localhost") || h.endsWith(".localhost") || allowed.contains(h)) {
                    return builtin.lookupByName(host, policy);
                }
                if (!localHost.isEmpty() && h.equals(localHost)) {
                    return Stream.of(InetAddress.getLoopbackAddress());
                }
                report(host);
                throw new UnknownHostException(host + ": blocked by the e2e harness");
            }

            @Override
            public String lookupByAddress(byte[] addr) throws UnknownHostException {
                InetAddress a = InetAddress.getByAddress(addr);
                if (a.isLoopbackAddress()) {
                    return builtin.lookupByAddress(addr);
                }
                // a local interface and the game server's own literal address have no reverse name: refusing without a
                // report makes the JDK keep the literal, with no DNS query and no forward lookup of it
                if (isLocal(a) || allowed.contains(a.getHostAddress())) {
                    throw new UnknownHostException(a.getHostAddress());
                }
                report(a.getHostAddress());
                throw new UnknownHostException(a.getHostAddress() + ": blocked by the e2e harness");
            }
        };
    }

    private static boolean isLocal(InetAddress a) {
        try {
            return NetworkInterface.getByInetAddress(a) != null;
        } catch (SocketException e) {
            return false;
        }
    }

    private static void report(String host) {
        System.out.println("[E2E-NET] {\"blocked\":\"" + host.replace("\\", "\\\\").replace("\"", "\\\"") + "\"}");
        System.out.flush();
    }

    @Override
    public String name() {
        return "e2e isolation";
    }
}
