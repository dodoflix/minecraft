import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import jdk.jfr.EventType;
import jdk.jfr.SettingDescriptor;
import jdk.jfr.ValueDescriptor;
import jdk.jfr.consumer.RecordedEvent;
import jdk.jfr.consumer.RecordingFile;
import net.minecraft.util.profiling.jfr.parse.JfrStatsParser;

/**
 * Reads a flight recording with the JDK's own reader, on the recording of the vanilla server and of the Rust server.
 *
 * <p>Usage: {@code scripts/vanilla-java.sh JfrOracle.java <types|summary|chunks> <file.jfr>}
 *
 * <p>Output: for {@code types}, what the Minecraft part of the recording declares: the event types (name, label,
 * description, categories), their settings and fields, and which of them have events, ending in {@code OK}; for
 * {@code summary}, vanilla's {@code JfrStatsParser} and {@code JfrResultJsonSerializer} on the recording (the JSON that
 * {@code /jfr stop} writes next to it); for {@code chunks}, the chunk region events in the order the parser reads them.
 */
public class JfrOracle {
    public static void main(String[] args) throws Exception {
        Path file = Path.of(args[1]);
        if (args[0].equals("summary")) {
            // the parser reads chunk statuses and resolves registries
            net.minecraft.SharedConstants.tryDetectVersion();
            net.minecraft.server.Bootstrap.bootStrap();
            System.out.print(JfrStatsParser.parse(file).asJson());
            return;
        }
        if (args[0].equals("chunks")) {
            try (RecordingFile recording = new RecordingFile(file)) {
                while (recording.hasMoreEvents()) {
                    RecordedEvent event = recording.readEvent();
                    String name = event.getEventType().getName();
                    if (name.startsWith("minecraft.ChunkRegion")) {
                        System.out.println(name + " " + event.getString("level") + " " + event.getString("dimension")
                            + " " + event.getInt("chunkPosX") + " " + event.getInt("chunkPosZ") + " "
                            + event.getInt("bytes") + " " + event.getEndTime());
                    }
                }
            }
            return;
        }
        Map<String, Long> counts = new TreeMap<>();
        try (RecordingFile recording = new RecordingFile(file)) {
            while (recording.hasMoreEvents()) {
                RecordedEvent event = recording.readEvent();
                counts.merge(event.getEventType().getName(), 1L, Long::sum);
            }
        }
        List<EventType> types;
        try (RecordingFile recording = new RecordingFile(file)) {
            types = recording.readEventTypes();
        }
        types = new ArrayList<>(types);
        types.sort(Comparator.comparing(EventType::getName));
        for (EventType type : types) {
            if (!type.getName().startsWith("minecraft.")) {
                continue;
            }
            System.out.println("TYPE " + type.getName());
            System.out.println("  label=" + type.getLabel());
            System.out.println("  description=" + type.getDescription());
            System.out.println("  categories=" + type.getCategoryNames());
            System.out.println("  events=" + (counts.getOrDefault(type.getName(), 0L) > 0 ? "some" : "none"));
            List<SettingDescriptor> settings = new ArrayList<>(type.getSettingDescriptors());
            settings.sort(Comparator.comparing(SettingDescriptor::getName));
            for (SettingDescriptor setting : settings) {
                System.out.println("  setting " + setting.getName() + " type=" + setting.getTypeName() + " label="
                    + setting.getLabel() + " default=" + setting.getDefaultValue());
            }
            for (ValueDescriptor field : type.getFields()) {
                System.out.println("  field " + field.getName() + " type=" + field.getTypeName() + " label="
                    + field.getLabel() + " description=" + field.getDescription() + " contentType="
                    + field.getContentType() + " array=" + field.isArray());
            }
        }
        System.out.println("OK");
    }
}
