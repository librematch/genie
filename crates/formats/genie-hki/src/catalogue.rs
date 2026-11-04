//! The known hotkey groups ([`HotkeyGroupId`]) and the hotkeys of each group, by index.

// TODO PCM III uses lots of hard coded changes for 2nd page hotkeys
// Test with that for resolving hotkey conflicts

/// Available hotkey groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HotkeyGroupId {
    /// Hotkey commands when a unit is selected.
    UnitCommands = 0x0,
    /// Commands to control the game (speed, pause, UI toggles, etc).
    GameCommands = 0x1,
    /// Map scrolling commands.
    Scroll = 0x2,
    /// Villager build commands.
    Villager = 0x3,
    /// Commands when a Town Center is selected.
    TownCenter = 0x4,
    /// Commands when a Dock is selected.
    Dock = 0x5,
    /// Commands when a Barracks is selected.
    Barracks = 0x6,
    /// Commands when an Archery Range is selected.
    ArcheryRange = 0x7,
    /// Commands when a Stable is selected.
    Stable = 0x8,
    /// Commands when a Siege Workshop is selected.
    SiegeWorkshop = 0x9,
    /// Commands when a Monastery is selected.
    Monastery = 0xA,
    /// Commands when a Market is selected.
    Market = 0xB,
    /// Commands when a military unit is selected.
    MilitaryUnits = 0xC,
    /// Commands when a Castle is selected.
    Castle = 0xD,
    /// Commands when a Mill is selected.
    Mill = 0xE,
}

/// Hotkeys for castles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CastleHotkeys {
    /// Create a Trebuchet.
    Trebuchet = 0x0,
    /// Create the unique unit.
    UniqueUnit = 0x1,
    /// Create a Petard.
    Petard = 0x2,
}

/// Hotkeys for military units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MilitaryUnitHotkeys {
    /// Formation command.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    Formation = 0x0,
    /// Wheel the formation left. The `AoC`, HD Edition and `WololoKingdoms` files hold the
    /// Box formation in this slot (see [`FormationHotkeys`]).
    WheelLeft = 0x1,
    /// Wheel the formation right. The `AoC`, HD Edition and `WololoKingdoms` files hold
    /// the Line formation in this slot (see [`FormationHotkeys`]).
    WheelRight = 0x2,
    /// Turn the formation around. The `AoC`, HD Edition and `WololoKingdoms` files hold
    /// the Staggered formation in this slot (see [`FormationHotkeys`]).
    AboutFace = 0x3,
    /// Break up the formation. The `AoC`, HD Edition and `WololoKingdoms` files hold
    /// the Flank formation in this slot (see [`FormationHotkeys`]).
    DisbandFormation = 0x4,
    /// Patrol.
    Patrol = 0x5,
    /// Guard a unit or building.
    Guard = 0x6,
    /// Follow a unit.
    Follow = 0x7,
    /// Set the aggressive stance.
    Aggressive = 0x8,
    /// Set the defensive stance.
    Defensive = 0x9,
    /// Set the stand ground stance.
    StandGround = 0xA,
    /// Set the no attack stance.
    NoAttack = 0xB,
    /// HD Edition only.
    AttackMove = 0xC,
}

/// Hotkeys to change formations.
///
/// They index the [`HotkeyGroupId::MilitaryUnits`] group, like [`MilitaryUnitHotkeys`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormationHotkeys {
    /// Horde formation.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    Horde = 0x0,
    /// Box formation.
    Box = 0x1,
    /// Line formation.
    Line = 0x2,
    /// Staggered formation.
    Staggered = 0x3,
    /// Flank formation.
    Flank = 0x4,
}

/// Hotkeys to control units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnitCommandHotkeys {
    /// Open the economic buildings a villager can build.
    BuildEconomic = 0x0,
    /// Open the military buildings a villager can build.
    BuildMilitary = 0x1,
    /// Repair.
    Repair = 0x2,
    /// Group the selected units.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    Group = 0x3,
    /// Ungroup the selected units.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    Ungroup = 0x4,
    /// Stop.
    Stop = 0x5,
    /// Unload a transport or ungarrison.
    Unload = 0x6,
    /// Pack a Trebuchet.
    Pack = 0x7,
    /// Unpack a Trebuchet.
    Unpack = 0x8,
    /// Heal (Monks).
    Heal = 0x9,
    /// Convert (Monks).
    Convert = 0xA,
    /// Garrison.
    Garrison = 0xB,
    /// Delete the selected unit.
    Delete = 0xC,
    /// Set the gather point of a building.
    SetGatherPoint = 0xD,
    /// Attack the ground.
    AttackGround = 0xE,
}

/// Hotkeys for the game UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GameCommandHotkeys {
    /// Create control group 0 (HD Edition shows it as #10).
    CreateGroup0 = 0x0,
    /// Create control group 1.
    CreateGroup1 = 0x1,
    /// Create control group 2.
    CreateGroup2 = 0x2,
    /// Create control group 3.
    CreateGroup3 = 0x3,
    /// Create control group 4.
    CreateGroup4 = 0x4,
    /// Create control group 5.
    CreateGroup5 = 0x5,
    /// Create control group 6.
    CreateGroup6 = 0x6,
    /// Create control group 7.
    CreateGroup7 = 0x7,
    /// Create control group 8.
    CreateGroup8 = 0x8,
    /// Create control group 9.
    CreateGroup9 = 0x9,
    /// Select control group 0 (HD Edition shows it as #10).
    SelectGroup0 = 0xA,
    /// Select control group 1.
    SelectGroup1 = 0xB,
    /// Select control group 2.
    SelectGroup2 = 0xC,
    /// Select control group 3.
    SelectGroup3 = 0xD,
    /// Select control group 4.
    SelectGroup4 = 0xE,
    /// Select control group 5.
    SelectGroup5 = 0xF,
    /// Select control group 6.
    SelectGroup6 = 0x10,
    /// Select control group 7.
    SelectGroup7 = 0x11,
    /// Select control group 8.
    SelectGroup8 = 0x12,
    /// Select control group 9.
    SelectGroup9 = 0x13,
    /// Send a chat message.
    Chat = 0x14,
    /// Speed up the game.
    SpeedUp = 0x15,
    /// Slow down the game.
    SpeedDown = 0x16,
    /// Go to the next idle villager.
    NextIdleVillager = 0x17,
    /// Go to the next idle villager (second binding).
    NextIdleVillager2 = 0x18,
    /// Go to the last notification.
    CycleFocusArea = 0x19,
    /// Go to the last notification (second binding).
    CycleFocusArea2 = 0x1A,
    /// Go to the selected object.
    GoToSelected = 0x1B,
    /// Go to a Town Center.
    GoToTownCenter = 0x1C,
    /// Go to a Town Center (second binding).
    GoToTownCenter2 = 0x1D,
    /// Go to a Market.
    GoToMarket = 0x1E,
    /// Review chat messages backward.
    ScrollChatDown = 0x1F,
    /// Review chat messages forward.
    ScrollChatUp = 0x20,
    /// Display the statistics.
    Score = 0x21,
    /// Go to a Barracks.
    GoToBarracks = 0x22,
    /// Go to an Archery Range.
    GoToArcheryRange = 0x23,
    /// Go to a Stable.
    GoToStable = 0x24,
    /// Go to a Siege Workshop.
    GoToSiegeWorkshop = 0x25,
    /// Go to a Dock.
    GoToDock = 0x26,
    /// Go to a Monastery.
    GoToMonastery = 0x27,
    /// Go to a Blacksmith.
    GoToBlacksmith = 0x28,
    /// Go to a Mill.
    GoToMill = 0x29,
    /// Go to a University.
    GoToUniversity = 0x2A,
    /// Display the technology tree.
    TechTree = 0x2B,
    /// Display the achievements.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    Achievements = 0x2C,
    /// Display the game time.
    DisplayGameTime = 0x2D,
    /// Go to the next idle military unit.
    NextIdleMilitary = 0x2E,
    /// Go to the next idle military unit (second binding).
    NextIdleMilitary2 = 0x2F,
    /// Send a flare.
    Flare = 0x30,
    /// Go to a Castle.
    GoToCastle = 0x31,
    /// Go to a Mining Camp.
    GoToMiningCamp = 0x32,
    /// Go to a Lumber Camp.
    GoToSawMill = 0x33,
    /// Switch the mini-map to combat mode.
    MinimapCombat = 0x34,
    /// Switch the mini-map to economic (resource) mode.
    MinimapResource = 0x35,
    /// Switch the mini-map to normal mode.
    MinimapNormal = 0x36,
    /// Toggle the extended help.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    ExtendedHelp = 0x37,
    /// Advanced modes.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    AdvancedModes = 0x38,
    /// Open the diplomacy screen.
    Diplomacy = 0x39,
    /// Open the game menu.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files
    /// (F10 is reserved for it).
    Menu = 0x3A,
    /// Open the objectives.
    Objectives = 0x3B,
    /// Open the chat dialog.
    ChatDialog = 0x3C,
    /// Pause the game.
    PauseGame = 0x3D,
    /// Save the game.
    SaveGame = 0x3E,
    /// Toggle friend or foe colours. Not in the original `AoC` files.
    FriendFoeColors = 0x3F,
    /// Return to the previous view. Not in the original `AoC` files.
    PreviousView = 0x40,
    /// Save a chapter. Not in the original `AoC` files.
    SaveChapter = 0x41,
}

/// Hotkeys for scrolling.
///
/// Only the first four are bound in the `AoC`, HD Edition and `WololoKingdoms` files; the others
/// have string id -1 there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScrollHotkeys {
    /// Scroll left.
    ScrollLeft = 0x0,
    /// Scroll right.
    ScrollRight = 0x1,
    /// Scroll up.
    ScrollUp = 0x2,
    /// Scroll down.
    ScrollDown = 0x3,
    /// Scroll left fast.
    FastScrollLeft = 0x4,
    /// Scroll right fast.
    FastScrollRight = 0x5,
    /// Scroll up fast.
    FastScrollUp = 0x6,
    /// Scroll down fast.
    FastScrollDown = 0x7,
    /// Scroll left and up.
    ScrollLeftUp = 0x8,
    /// Scroll left and down.
    ScrollLeftDown = 0x9,
    /// Scroll right and down.
    ScrollRightDown = 0xA,
    /// Scroll right and up.
    ScrollRightUp = 0xB,
    /// Scroll left and up fast.
    FastScrollLeftUp = 0xC,
    /// Scroll left and down fast.
    FastScrollLeftDown = 0xD,
    /// Scroll right and down fast.
    FastScrollRightDown = 0xE,
    /// Scroll right and up fast.
    FastScrollRightUp = 0xF,
}

/// Hotkeys for villagers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VillagerHotkeys {
    /// Build a House.
    BuildHouse = 0x0,
    /// Build a Mill.
    BuildMill = 0x1,
    /// Build a Blacksmith.
    BuildBlacksmith = 0x2,
    /// Build a Dock.
    BuildDock = 0x3,
    /// Build a Barracks.
    BuildBarracks = 0x4,
    /// Build a Palisade Wall.
    BuildPalisadeWall = 0x5,
    /// Build a Market.
    BuildMarket = 0x6,
    /// Build a Stone Wall.
    BuildStoneWall = 0x7,
    /// Build a tower.
    BuildTower = 0x8,
    /// Build a Bombard Tower.
    BuildBombardTower = 0x9,
    /// Build a Gate.
    BuildGate1 = 0xA,
    /// Build a Gate (second slot).
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    BuildGate2 = 0xB,
    /// Build a Farm.
    BuildFarm = 0xC,
    /// Build an Archery Range.
    BuildArcheryRange = 0xD,
    /// Build a Stable.
    BuildStable = 0xE,
    /// Build a Monastery.
    BuildMonastery = 0xF,
    /// Build a Town Center.
    BuildTownCenter = 0x10,
    /// Build a Siege Workshop.
    BuildSiegeWorkshop = 0x11,
    /// Build a University.
    BuildUniversity = 0x12,
    /// Build a Wonder.
    BuildWonder = 0x13,
    /// Build a Castle.
    BuildCastle = 0x14,
    /// Build a Trade Workshop.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    BuildTradeWorkshop = 0x15,
    /// Build a packed Town Center.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    BuildPackedTownCenter = 0x16,
    /// Build a Lumber Camp.
    BuildSawMill = 0x17,
    /// Build a Mining Camp.
    BuildMiningCamp = 0x18,
    /// Build a Fish Trap.
    BuildFishTrap = 0x19,
    /// Build an Outpost.
    BuildOutpost = 0x1A,
    /// Show the next page of buildings.
    BuildNext = 0x1B,
    /// Build a Palisade Gate. Only in the `WololoKingdoms` file.
    BuildPalisadeGate = 0x1C,
    /// Build a Feitoria. Only in the `WololoKingdoms` file.
    BuildFeitoria = 0x1D,
}

/// Hotkeys for the town center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TownCenterHotkeys {
    /// Train a villager.
    CreateVillager = 0x0,
    /// Advance to the next age.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    AgeAdvance = 0x1,
    /// Named for a raider archer.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    CreateRaiderArcher = 0x2,
    /// Named for a raider swordsman.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    CreateRaiderSwordsman = 0x3,
    /// Named for a raider cavalry unit.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    CreateRaiderCavalry = 0x4,
    /// Named for a raider cavalry archer.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    CreateRaiderCavArcher = 0x5,
    /// Ring the town bell.
    RingTownBell = 0x6,
    /// Send the garrisoned villagers back to work.
    BackToWork = 0x7,
}

/// Hotkeys for mills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MillHotkeys {
    /// Queue a farm reseed.
    AddFarm = 0x0,
}

/// Hotkeys for docks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DockHotkeys {
    /// Build a Fishing Ship.
    CreateFishingShip = 0x0,
    /// Build a Trade Cog.
    CreateTradeCog = 0x1,
    /// Build a Galley (or its upgrade).
    CreateGalley = 0x2,
    /// Build a Cannon Galleon.
    CreateCannonGalleon = 0x3,
    /// Build a Fire Ship.
    CreateFireGalley = 0x4,
    /// Build a Demolition Ship.
    CreateDemolitionShip = 0x5,
    /// Build a boarding galley.
    /// Unused (string id -1) in the `AoC`, HD Edition and `WololoKingdoms` files.
    CreateBoardingGalley = 0x6,
    /// Build a Transport Ship.
    CreateTransport = 0x7,
    /// Build a Longboat.
    CreateLongboat = 0x8,
    /// Build a Turtle Ship. Not in the original `AoC` files.
    CreateTurtleShip = 0x9,
}

/// Hotkeys for the barracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BarracksHotkeys {
    /// Train a Militia (or its upgrade).
    CreateMilitia = 0x0,
    /// Train a Spearman (or its upgrade).
    CreatePikeman = 0x1,
    /// Train an Eagle Warrior. Not in the original `AoC` files.
    CreateEagleWarrior = 0x2,
    /// Train a Huskarl. Not in the original `AoC` files.
    CreateHuskarl = 0x3,
}

/// Hotkeys for archery ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArcheryRangeHotkeys {
    /// Train an Archer (or its upgrade).
    CreateArcher = 0x0,
    /// Train a Skirmisher.
    CreateSkirmisher = 0x1,
    /// Train a Cavalry Archer.
    CreateCavArcher = 0x2,
    /// Train a Hand Cannoneer.
    CreateHandCannoneer = 0x3,
    /// Train a Genitour. Only in the `WololoKingdoms` file.
    CreateGenitour = 0x4,
}

/// Hotkeys for stables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StableHotkeys {
    /// Train a Scout Cavalry (or its upgrade).
    CreateScout = 0x0,
    /// Train a Camel.
    CreateCamel = 0x1,
    /// Train a Knight (or its upgrade).
    CreateKnight = 0x2,
    /// Train a Battle Elephant. Not in the `AoC`, HD Edition and
    /// `WololoKingdoms` files.
    CreateBattleElephant = 0x3,
}

/// Hotkeys for siege workshops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SiegeWorkshopHotkeys {
    /// Build a Battering Ram.
    CreateRam = 0x0,
    /// Build a Scorpion.
    CreateScorpion = 0x1,
    /// Build a Mangonel.
    CreateMangonel = 0x2,
    /// Build a Bombard Cannon.
    CreateBombardCannon = 0x3,
    /// Build a Siege Tower. Only in the `WololoKingdoms` file.
    CreateSiegeTower = 0x4,
}

/// Hotkeys for the monastery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MonasteryHotkeys {
    /// Train a Monk.
    CreateMonk = 0x0,
    /// Train a Missionary. Not in the original `AoC` files.
    CreateMissionary = 0x1,
}

/// Hotkeys for the market.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarketHotkeys {
    /// Build a Trade Cart.
    CreateTradeCart = 0x0,
}

/// Hotkeys for the blacksmith.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlacksmithHotkeys {}
