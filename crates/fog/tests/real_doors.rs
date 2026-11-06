//! The doors read from the player's own game (`APOC_ER_STOCK`; skipped when unset).

use std::path::PathBuf;

use fog::doors;
use gamedata::Sources;

#[test]
fn boss_fogs_and_mouths_are_found() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let fogs = doors::boss_fogs(&s);
    let mouths = doors::mouths(&s);
    eprintln!("{} boss fogs, {} mouths", fogs.len(), mouths.len());
    // Margit's fog, its arena region 1.2 m past it.
    let margit = fogs.iter().find(|d| d.fog == Some(10_001_850)).unwrap();
    assert_eq!(margit.inside.entity, Some(10_002_850));
    let gap = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    assert!(gap(margit.wall.position, margit.inside.position) < 3.0);
    assert!((gap(margit.wall.position, margit.outside.position) - doors::STEP).abs() < 0.01);
    assert!((100..=125).contains(&fogs.len()), "{}", fogs.len());
    // Stormfoot Catacombs' mouth.
    let stormfoot = mouths
        .iter()
        .find(|d| d.map == gamedata::MapId([30, 2, 0, 0]))
        .unwrap();
    assert_eq!(stormfoot.outside.entity, Some(30_022_840));
    // 54 on 2.7.2: the dungeons whose `AABB2840` is a return point (type 8) and that have a
    // grace; Murkwater Cave, Darklight Catacombs and a few others have no return point,
    // and in five more (Lakeside Crystal Cave, Gaol Cave…) 2840 is something else.
    assert!((50..=60).contains(&mouths.len()), "{}", mouths.len());
}
