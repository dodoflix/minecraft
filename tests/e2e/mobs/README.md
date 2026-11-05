# Mob end-to-end scenarios: the same bot script against vanilla and our server

Two oracles, two jobs:

| | in-process oracle (`scripts/entity-oracle.sh`) | dual-server e2e (this directory) |
|---|---|---|
| what runs | real vanilla `ServerLevel` ticked by hand, scenario DSL (`crates/mc-game/tests/oracle/scenarios`) | real vanilla server vs `mc-server`, a headless bot over the wire |
| what is compared | exact server-side state (mob fields, paths, goals, RNG-driven choices) | what a *client* can observe: packets, entity metadata, positions, offers, chat |
| use it for | goldens of mob logic (fixtures + `run_scenario_test`) | protocol-level behaviour: spawn/metadata/animation packets, interaction flow, menus, timing |

## Recipe (one scenario, both servers, sequentially)

```sh
scripts/cargo-locked.sh build -p mc-testbot -p mc-server        # once (binaries under target/debug)
scripts/mob-e2e.sh tests/e2e/mobs/summon_mobs.scn               # vanilla, then ours, then diff
scripts/mob-e2e.sh tests/e2e/mobs/combat.scn --sides vanilla    # only record the oracle (run/mob-e2e/combat/vanilla.{jsonl,obs})
scripts/mob-e2e.sh tests/e2e/mobs/breeding.scn --ignore update_attributes --ignore set_entity_data   # drop noisy packets from the diff
```

Outputs in `run/mob-e2e/NAME/`: `vanilla.jsonl`/`ours.jsonl` (full packet logs of the bot), `vanilla.obs`/`ours.obs` (the `observe` lines),
`diff.txt` (`mc-testbot diff`, ids are never compared, see `--field`/`--all-fields`) and `obs.diff`. The servers are started one after the
other (never two JVMs/servers at once) with identical properties (flat world, normal difficulty, no natural spawning, survival, bot op'd).
By hand: `scripts/vanilla-server.sh start ...`, `mc-testbot run 127.0.0.1:PORT tests/e2e/mobs/X.scn --log a.jsonl --obs a.txt`,
`mc-testbot diff a.jsonl b.jsonl --state play`. The Rust dual-test facility (`crates/mc-testbot/tests/entity_dual.rs`, scenarios written in Rust
with committed vanilla observations) is the right place for assertions that must run in CI; `.scn` files are for exploration and diffing.

## Scenario language (`crates/mc-testbot/src/scenario.rs`)

`cmd /summon ...` (the bot is op'd: any vanilla command, also `@s`), `chat`, `wait_ticks N`, `wait SECONDS`, `wait_chat TEXT`, `look_at X Y Z`,
`walk X Z [sprint]`, `jump`, `sneak on|off`, `select N`, `wait_entity TYPE [S]` (waits for the `add_entity` packet of a new entity of that
type), `attack TYPE#N`, `interact TYPE#N [main|off]` (right click: feeding, taming, opening the villager menu), `use_item_on X Y Z FACE`,
`use`, `trade_select N` (ServerboundSelectTrade), `observe LABEL`, `note`. `TYPE#N` is the N-th (1-based, ascending id) tracked entity of the type.
`observe` prints `PLAYER`, `ENT type#n pos= vel= rot= head= og= vehicle= data=<accessor id:value;...>` (entity metadata exactly as the
client got it) and `OFFERS`/`OFFER` lines of the last `merchant_offers` packet.

## Scenarios

| file | what it shows |
|---|---|
| `template.scn` | copy this: quiet frozen world, summon, wait for the spawn packet, observe |
| `summon_mobs.scn` | `/summon` of pig/sheep/baby zombie/baby cow/villager/powered creeper: initial metadata, rotation, variants |
| `darkness_spawning.scn` | natural spawning at night (hard), what the client sees after 400/800 ticks (kind/count loosely; exact placement is the oracle's `spawnpass`) |
| `combat.scn` | attack packets, damage events, knockback, death, a chasing zombie |
| `breeding.scn` | feeding wheat via interact, love event, baby spawn, parents' ages |
| `villager_trading.scn` | interact opens the merchant menu, offers, trade selection |
| `raid_start.scn` | village by POI blocks, raid omen, raid bar / raiders |

Server-side exact state (paths, goals, memories, RNG-seeded trades, spawn passes, raid waves) is covered by the in-process oracle scenarios
`crates/mc-game/tests/oracle/scenarios/mob_*.txt` (fixtures in `tests/fixtures/entity_oracle`). Timing differs between runs of a *real*
server (no per-tick reseeding like the oracle), so compare structure/metadata/positions with tolerance; the diff tool ignores packet ids.

## Adding a scenario

1. `cp template.scn mything.scn`, edit (all state changes through `cmd`, observations through `observe`).
2. `scripts/mob-e2e.sh tests/e2e/mobs/mything.scn --sides vanilla` and read `run/mob-e2e/mything/vanilla.obs`.
3. Run both sides; refine `--ignore`/`--field` until only real differences remain; fix our server.
