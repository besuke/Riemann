// Connects R's loading of the package to the routines registered by extendr.
void R_init_riemann_extendr(void *dll);

void R_init_riemann(void *dll) {
    R_init_riemann_extendr(dll);
}
