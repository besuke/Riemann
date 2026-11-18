// Connects R's loading of the package to the routines registered by extendr.
void R_init_rmql_extendr(void *dll);

void R_init_rmql(void *dll) {
    R_init_rmql_extendr(dll);
}
