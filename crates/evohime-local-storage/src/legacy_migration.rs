//! Ordered legacy schema migration orchestration.
//!
//! The installers themselves live in [`crate::migrations`]. This module keeps
//! the single transaction boundary and historical order out of `lib.rs`.

use rusqlite::Connection;

use crate::{migrations, StorageError};

pub(crate) fn run(
    connection: &Connection,
    current: u32,
    fail_migration: bool,
) -> Result<(), StorageError> {
    let transaction = connection.unchecked_transaction()?;
    migrations::v001::apply(&transaction, current)?;
    if fail_migration {
        return Err(rusqlite::Error::InvalidQuery.into());
    }
    migrations::v002::apply(&transaction, current)?;
    migrations::v003::apply(&transaction, current)?;
    migrations::v004::apply(&transaction, current)?;
    migrations::v005::apply(&transaction, current)?;
    migrations::v006::apply(&transaction, current)?;
    migrations::v007::apply(&transaction, current)?;
    migrations::v008::apply(&transaction, current)?;
    migrations::v009::apply(&transaction, current)?;
    migrations::v010::apply(&transaction, current)?;
    migrations::v011::apply(&transaction, current)?;
    migrations::v012::apply(&transaction, current)?;
    migrations::v013::apply(&transaction, current)?;
    migrations::v014::apply(&transaction, current)?;
    migrations::v015::apply(&transaction, current)?;
    migrations::v016::apply(&transaction, current)?;
    migrations::v017::apply(&transaction, current)?;
    migrations::v018::apply(&transaction, current)?;
    migrations::v019::apply(&transaction, current)?;
    migrations::v020::apply(&transaction, current)?;
    migrations::v021::apply(&transaction, current)?;
    migrations::v022::apply(&transaction, current)?;
    migrations::v023::apply(&transaction, current)?;
    migrations::v024::apply(&transaction, current)?;
    migrations::v025::apply(&transaction, current)?;
    migrations::v026::apply(&transaction, current)?;
    migrations::v032::apply(&transaction, current)?;
    migrations::v033::apply(&transaction, current)?;
    migrations::v034::apply(&transaction, current)?;
    migrations::v035::apply(&transaction, current)?;
    migrations::v036::apply(&transaction, current)?;
    migrations::v037::apply(&transaction, current)?;
    migrations::v038::apply(&transaction, current)?;
    migrations::v039::apply(&transaction, current)?;
    migrations::v042::apply(&transaction, current)?;
    migrations::v043::apply(&transaction, current)?;
    migrations::v044::apply(&transaction, current)?;
    migrations::v045::apply(&transaction, current)?;
    migrations::v046::apply(&transaction, current)?;
    migrations::v047::apply(&transaction, current)?;
    migrations::v048::apply(&transaction, current)?;
    migrations::v049::apply(&transaction, current)?;
    migrations::v050::apply(&transaction, current)?;
    migrations::v051::apply(&transaction, current)?;
    migrations::v052::apply(&transaction, current)?;
    migrations::v053::apply(&transaction, current)?;
    migrations::v054::apply(&transaction, current)?;
    migrations::v055::apply(&transaction, current)?;
    migrations::v056::apply(&transaction, current)?;
    migrations::v057::apply(&transaction, current)?;
    migrations::v058::apply(&transaction, current)?;
    migrations::v059::apply(&transaction, current)?;
    migrations::v060::apply(&transaction, current)?;
    migrations::v061::apply(&transaction, current)?;
    migrations::v062::apply(&transaction, current)?;
    migrations::v063::apply(&transaction, current)?;
    migrations::v064::apply(&transaction, current)?;
    migrations::v065::apply(&transaction, current)?;
    migrations::v066::apply(&transaction, current)?;
    migrations::v067::apply(&transaction, current)?;
    migrations::v068::apply(&transaction, current)?;
    migrations::v069::apply(&transaction, current)?;
    migrations::v070::apply(&transaction, current)?;
    migrations::v071::apply(&transaction, current)?;
    migrations::v072::apply(&transaction, current)?;
    migrations::v073::apply(&transaction, current)?;
    migrations::v074::apply(&transaction, current)?;
    migrations::v075::apply(&transaction, current)?;
    migrations::v076::apply(&transaction, current)?;
    migrations::v077::apply(&transaction, current)?;
    migrations::v078::apply(&transaction, current)?;
    migrations::v079::apply(&transaction, current)?;
    migrations::v080::apply(&transaction, current)?;
    migrations::v081::apply(&transaction, current)?;
    migrations::v082::apply(&transaction, current)?;
    migrations::v083::apply(&transaction, current)?;
    migrations::v084::apply(&transaction, current)?;
    migrations::v085::apply(&transaction, current)?;
    migrations::v086::apply(&transaction, current)?;
    migrations::v087::apply(&transaction, current)?;
    migrations::v088::apply(&transaction, current)?;
    migrations::v089::apply(&transaction, current)?;
    migrations::v090::apply(&transaction, current)?;
    migrations::v091::apply(&transaction, current)?;
    migrations::v092::apply(&transaction, current)?;
    migrations::v093::apply(&transaction, current)?;
    migrations::v094::apply(&transaction, current)?;
    migrations::v095::apply(&transaction, current)?;
    migrations::v096::apply(&transaction, current)?;
    migrations::v097::apply(&transaction, current)?;
    migrations::v098::apply(&transaction, current)?;
    migrations::v099::apply(&transaction, current)?;
    migrations::v100::apply(&transaction, current)?;
    migrations::v101::apply(&transaction, current)?;
    migrations::v102::apply(&transaction, current)?;
    migrations::v103::apply(&transaction, current)?;
    migrations::v104::apply(&transaction, current)?;
    migrations::v105::apply(&transaction, current)?;
    migrations::v106::apply(&transaction, current)?;
    migrations::v107::apply(&transaction, current)?;
    migrations::v108::apply(&transaction, current)?;
    migrations::v109::apply(&transaction, current)?;
    migrations::v110::apply(&transaction, current)?;
    migrations::v111::apply(&transaction, current)?;
    migrations::v112::apply(&transaction, current)?;
    migrations::v113::apply(&transaction, current)?;
    migrations::v114::apply(&transaction, current)?;
    migrations::v115::apply(&transaction, current)?;
    migrations::v116::apply(&transaction, current)?;
    migrations::v149::apply(&transaction, current)?;
    migrations::v150::apply(&transaction, current)?;
    migrations::v151::apply(&transaction, current)?;
    migrations::v152::apply(&transaction, current)?;
    migrations::v153::apply(&transaction, current)?;
    migrations::v154::apply(&transaction, current)?;
    migrations::v155::apply(&transaction, current)?;
    migrations::v156::apply(&transaction, current)?;
    migrations::v157::apply(&transaction, current)?;
    migrations::v158::apply(&transaction, current)?;
    migrations::v159::apply(&transaction, current)?;
    migrations::v160::apply(&transaction, current)?;
    migrations::v161::apply(&transaction, current)?;
    migrations::v162::apply(&transaction, current)?;
    migrations::v163::apply(&transaction, current)?;
    migrations::v164::apply(&transaction, current)?;
    migrations::v165::apply(&transaction, current)?;
    migrations::v166::apply(&transaction, current)?;
    migrations::v167::apply(&transaction, current)?;
    migrations::v168::apply(&transaction, current)?;
    migrations::v169::apply(&transaction, current)?;
    migrations::v170::apply(&transaction, current)?;
    migrations::v171::apply(&transaction, current)?;
    migrations::v172::apply(&transaction, current)?;
    migrations::v173::apply(&transaction, current)?;
    migrations::v174::apply(&transaction, current)?;
    migrations::v175::apply(&transaction, current)?;
    transaction.commit()?;
    Ok(())
}
