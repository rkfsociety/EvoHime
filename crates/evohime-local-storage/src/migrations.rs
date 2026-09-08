//! Schema evolution boundary for the local database.
//!
//! New schema changes belong in this module as numbered, small installers.
//! Historical schema transitions are kept in one file per version so the
//! migration order is explicit and reviewable. `LocalDatabase` owns the
//! transaction/backup boundary and delegates each installer to this module.

#![allow(dead_code)]

use rusqlite::Connection;

use crate::{LocalDatabase, StorageError};

pub(crate) mod v001;
pub(crate) mod v002;
pub(crate) mod v003;
pub(crate) mod v004;
pub(crate) mod v005;
pub(crate) mod v006;
pub(crate) mod v007;
pub(crate) mod v008;
pub(crate) mod v009;
pub(crate) mod v010;
pub(crate) mod v011;
pub(crate) mod v012;
pub(crate) mod v013;
pub(crate) mod v014;
pub(crate) mod v015;
pub(crate) mod v016;
pub(crate) mod v017;
pub(crate) mod v018;
pub(crate) mod v019;
pub(crate) mod v020;
pub(crate) mod v021;
pub(crate) mod v022;
pub(crate) mod v023;
pub(crate) mod v024;
pub(crate) mod v025;
pub(crate) mod v026;
pub(crate) mod v032;
pub(crate) mod v033;
pub(crate) mod v034;
pub(crate) mod v035;
pub(crate) mod v036;
pub(crate) mod v037;
pub(crate) mod v038;
pub(crate) mod v039;
pub(crate) mod v042;
pub(crate) mod v043;
pub(crate) mod v044;
pub(crate) mod v045;
pub(crate) mod v046;
pub(crate) mod v047;
pub(crate) mod v048;
pub(crate) mod v049;
pub(crate) mod v050;
pub(crate) mod v051;
pub(crate) mod v052;
pub(crate) mod v053;
pub(crate) mod v054;
pub(crate) mod v055;
pub(crate) mod v056;
pub(crate) mod v057;
pub(crate) mod v058;
pub(crate) mod v059;
pub(crate) mod v060;
pub(crate) mod v061;
pub(crate) mod v062;
pub(crate) mod v063;
pub(crate) mod v064;
pub(crate) mod v065;
pub(crate) mod v066;
pub(crate) mod v067;
pub(crate) mod v068;
pub(crate) mod v069;
pub(crate) mod v070;
pub(crate) mod v071;
pub(crate) mod v072;
pub(crate) mod v073;
pub(crate) mod v074;
pub(crate) mod v075;
pub(crate) mod v076;
pub(crate) mod v077;
pub(crate) mod v078;
pub(crate) mod v079;
pub(crate) mod v080;
pub(crate) mod v081;
pub(crate) mod v082;
pub(crate) mod v083;
pub(crate) mod v084;
pub(crate) mod v085;
pub(crate) mod v086;
pub(crate) mod v087;
pub(crate) mod v088;
pub(crate) mod v089;
pub(crate) mod v090;
pub(crate) mod v091;
pub(crate) mod v092;
pub(crate) mod v093;
pub(crate) mod v094;
pub(crate) mod v095;
pub(crate) mod v096;
pub(crate) mod v097;
pub(crate) mod v098;
pub(crate) mod v099;

pub(crate) fn run(
    connection: &Connection,
    current: u32,
    fail_migration: bool,
) -> Result<(), StorageError> {
    LocalDatabase::migrate_legacy(connection, current, fail_migration)
}
