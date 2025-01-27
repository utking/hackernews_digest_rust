use diesel::SqliteConnection;

use crate::{config::AppConfig, establish_connection, run_migrations};

pub struct Vacuum {
    config: AppConfig,
}

impl Vacuum {
    #[must_use]
    pub fn new(config: &AppConfig) -> Self {
        Vacuum {
            config: config.clone(),
        }
    }

    pub fn run(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let mut conn = establish_connection(&self.config.get_db_file());

        match run_migrations(&mut conn) {
            Ok(()) => {}
            Err(e) => eprintln!("Error running migrations: {e}"),
        }

        let num_deleted = self.vacuum(&mut conn)?;
        Ok(num_deleted)
    }

    fn vacuum(&self, conn: &mut SqliteConnection) -> Result<usize, Box<dyn std::error::Error>> {
        let num_deleted = crate::vacuum(self.config.purge_after_days, conn)?;
        Ok(num_deleted)
    }
}
