# Usage

```sh
./data/download-steam-depots.sh <steamusername>

cargo run --release
- 8384590172287463475 (2025-08-29)
- 6701825740120558137 (2025-09-10)

cargo run --release iff 8384590172287463475 6701825740120558137

./diff
└── '2025-08-29 to 2025-09-10'
    └── 'Hollow Knight Silksong_Data'
        ├── boot.config.diff
        ├── globalgamemanagers.assets.diff
        ├── globalgamemanagers.diff
        ├── Managed
        │   ├── Assembly-CSharp-firstpass.dll
        │   ├── Assembly-CSharp.dll
        │   └── TeamCherry.BuildBot.dll
        ├── resources.assets.diff
        └── StreamingAssets
            ├── aa
            │   ├── AddressablesLink
            │   │   └── link.xml.diff
            │   ├── catalog.bin
            │   ├── catalog.hash.diff
            │   ├── settings.json.diff
            │   └── StandaloneLinux64
            │       ├── 94696d22b6ed0a74097d1bd58feb4dce_monoscripts.bundle.diff
            │       ├── atlases_assets_assets
            │       │   └── sprites
            │       │       └── _atlases
            │       │           ├── abyss.spriteatlas.bundle.diff
            │       │           ├── abyss_last_dive.spriteatlas.bundle.diff
            │       │           └──  ...
            │       ├── coremanagers_assets__uimanager.bundle.diff
            │       ├── dataassets_assets_assets
            │       │   └── dataassets
            │       │       ├── collectables
            │       │       │   └── collectableitems.bundle.diff
            │       │       ├── costs.bundle.diff
            │       │       ├── enemyjournal
            │       │       │   └── journalrecords.bundle.diff
            │       │       ├── questsystem
            │       │       │   └── quests.bundle.diff
            │       │       └── shopitems.bundle.diff
            │       ├── enemycorpses_assets_areasong.bundle.diff
            │       ├── fonts_assets_.bundle.diff
            │       ├── globalsettings_assets_all.bundle.diff
            │       ├── maps_assets_all.bundle.diff
            │       ├── ...
            │       ├── scenes_scenes_scenes
            │       │   ├── abyss_cocoon.bundle.diff
            │       │   ├── ant_02.bundle.diff
            │       │   ├── ant_03.bundle.diff
            │       │   ├── ant_04.bundle.diff
            │       │   ├── ant_04_left.bundle.diff
            │       │   ├── ant_04_mid.bundle.diff
            │       │   └──  ...
            │       └── thief_assets_all.bundle.diff
            └── BuildMetadata.json.diff
```

**dataassets_assets_assets/dataassets/collectables/collectableitems.bundle.diff**
```diff
--- changed MonoBehaviour CollectableItemRelicType 'R Weaver Record' ---
.rewardAmount 110 -> 210
--- changed MonoBehaviour CollectableItemRelicType 'R Psalm Cylinder' ---
.rewardAmount 100 -> 200
--- changed MonoBehaviour CollectableItemRelicType 'R Weaver Totem' ---
.rewardAmount 75 -> 150
--- changed MonoBehaviour CollectableItemRelicType 'R Librarian Melody Cylinder' ---
.rewardAmount 200 -> 320
--- changed MonoBehaviour CollectableItemRelicType 'R Seal Chit' ---
.rewardAmount 90 -> 180
--- changed MonoBehaviour CollectableItemRelicType 'R Bone Record' ---
.rewardAmount 50 -> 90
--- changed MonoBehaviour CollectableItemRelicType 'R Ancient Egg' ---
.rewardAmount 250 -> 600
```
