fn main() {
    my_app_db::ensure_linked();
    dibs::build_queries(".dibs-queries/queries.styx");
}
