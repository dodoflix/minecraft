// Prints authlib's hasJoined query strings (ad-hoc query tool, no fixture is written).
// Usage: scripts/vanilla-java.sh crates/mc-protocol/tests/fixtures/AuthQuery.java
// Output: stdout only
import com.mojang.authlib.HttpDiscoveryService;
import java.util.*;
public class Q { public static void main(String[] a){
 Map<String,Object> m=new HashMap<>(); m.put("username","Nötch *~ a+b"); m.put("serverId","-1a2b"); m.put("ip","127.0.0.1");
 System.out.println(HttpDiscoveryService.buildQuery(m));
 m.remove("ip"); System.out.println(HttpDiscoveryService.buildQuery(m));
}}
