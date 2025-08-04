pub mod client;
pub mod config;
pub(crate) mod grpc;
pub(crate) mod middleware;
pub mod server;
pub(crate) mod sysinfo;
pub mod systemd;
pub(crate) mod zfs;

// dirty af but it works
#[cfg(feature = "test")]
pub mod testutil;

#[cfg(test)]
pub mod testutil;

#[cfg(test)]
#[cfg(feature = "zfs")]
#[test]
fn zfs_warning() {
    if cfg!(feature = "zfs") {
        println!();
        println!("--- WARNING: PLEASE READ ---");
        println!();
        println!("The ZFS tests perform CRUD operations against zpools and ZFS datasets/volumes.");
        println!("You must have ZFS support on your host.");
        println!("Pools are created from empty files and datasets/volumes created within,");
        println!("but there is no guarantee the code is correct. Please be mindful of");
        println!("your personal filesystems when running this code.");
        println!();
        println!("It must be run as root, otherwise these tests will fail on permissions.");
        println!();
        println!("--- WARNING: PLEASE READ ---");
        println!();
    }
}
