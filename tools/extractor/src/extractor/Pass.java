package extractor;

/** One extract pass: reads vanilla code-defined data and writes one or more JSON files through {@link Ctx}. */
public interface Pass {
   String name();

   void run(Ctx ctx) throws Exception;
}
