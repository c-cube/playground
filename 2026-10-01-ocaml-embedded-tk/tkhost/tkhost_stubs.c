/* The only C in tkhost: start Tcl/Tk, hand it our socketpair end, register
   `ocaml::cmd`, run the main loop.

   Everything Tcl runs with the OCaml runtime released, so the reader and
   worker threads keep going; `ocaml::cmd` re-acquires it to call back into
   OCaml. */

#define CAML_NAME_SPACE
#include <caml/alloc.h>
#include <caml/callback.h>
#include <caml/fail.h>
#include <caml/memory.h>
#include <caml/mlvalues.h>
#include <caml/threads.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <tcl.h>

/* not <tk.h>: its X11 headers clash with OCaml's Atom macro */
extern int Tk_Init(Tcl_Interp *interp);
extern void Tk_MainLoop(void);

/* `ocaml::cmd name ?arg ...?` calls the OCaml closure registered as
   "tkhost_cmd" : string array -> bool * string (ok, result or error). */
static int cmd_proc(ClientData cd, Tcl_Interp *interp, int objc, Tcl_Obj *const objv[]) {
  int code;
  caml_acquire_runtime_system();
  {
    CAMLparam0();
    CAMLlocal3(args, res, s);
    const value *f = caml_named_value("tkhost_cmd");
    args = caml_alloc(objc - 1, 0);
    for (int i = 1; i < objc; i++) {
      int len;
      const char *p = Tcl_GetStringFromObj(objv[i], &len);
      Store_field(args, i - 1, caml_alloc_initialized_string(len, p));
    }
    res = caml_callback_exn(*f, args);
    if (Is_exception_result(res)) {
      code = TCL_ERROR;
      Tcl_SetObjResult(interp, Tcl_NewStringObj("ocaml::cmd: uncaught OCaml exception", -1));
    } else {
      code = Bool_val(Field(res, 0)) ? TCL_OK : TCL_ERROR;
      s = Field(res, 1);
      Tcl_SetObjResult(interp, Tcl_NewStringObj(String_val(s), caml_string_length(s)));
    }
    CAMLdrop;
  }
  caml_release_runtime_system();
  return code;
}

/* malloc'd "what: errorInfo" */
static char *tcl_error(Tcl_Interp *interp, const char *what) {
  const char *info = Tcl_GetVar2(interp, "errorInfo", NULL, TCL_GLOBAL_ONLY);
  if (info == NULL) info = Tcl_GetStringResult(interp);
  size_t n = strlen(what) + strlen(info) + 3;
  char *msg = malloc(n);
  snprintf(msg, n, "%s: %s", what, info);
  return msg;
}

static Tcl_Obj *obj_of_string(value s) {
  Tcl_Obj *o = Tcl_NewStringObj(String_val(s), caml_string_length(s));
  Tcl_IncrRefCount(o);
  return o;
}

/* tkhost_run : string (argv0) -> Unix.file_descr -> string (files) -> string (prelude) -> unit
   Raises Failure if Tcl/Tk can't start or the prelude fails. */
value tkhost_run(value argv0, value fd, value files, value prelude) {
  CAMLparam4(argv0, fd, files, prelude);
  /* copy what we need out of the OCaml heap before releasing the runtime */
  char *exe = strdup(String_val(argv0));
  int ifd = Int_val(fd);
  Tcl_Obj *files_obj = obj_of_string(files);
  Tcl_Obj *prelude_obj = obj_of_string(prelude);
  char *err = NULL;

  caml_release_runtime_system();
  Tcl_FindExecutable(exe);
  Tcl_Interp *interp = Tcl_CreateInterp();
  if (Tcl_Init(interp) != TCL_OK) {
    err = tcl_error(interp, "Tcl_Init");
  } else if (Tk_Init(interp) != TCL_OK) {
    err = tcl_error(interp, "Tk_Init");
  } else {
    /* Tcl owns the fd from now on */
    Tcl_Channel chan = Tcl_MakeFileChannel((ClientData)(intptr_t)ifd, TCL_READABLE | TCL_WRITABLE);
    Tcl_RegisterChannel(interp, chan);
    Tcl_Eval(interp, "namespace eval ocaml {}");
    Tcl_SetVar2Ex(interp, "::ocaml::chan", NULL, Tcl_NewStringObj(Tcl_GetChannelName(chan), -1), TCL_GLOBAL_ONLY);
    Tcl_SetVar2Ex(interp, "::ocaml::files", NULL, files_obj, TCL_GLOBAL_ONLY);
    Tcl_CreateObjCommand(interp, "ocaml::cmd", cmd_proc, NULL, NULL);
    if (Tcl_EvalObjEx(interp, prelude_obj, TCL_EVAL_GLOBAL) != TCL_OK) {
      err = tcl_error(interp, "tkhost prelude");
    } else {
      Tk_MainLoop();
    }
  }
  Tcl_DecrRefCount(files_obj);
  Tcl_DecrRefCount(prelude_obj);
  free(exe);
  caml_acquire_runtime_system();

  if (err != NULL) {
    value msg = caml_copy_string(err);
    free(err);
    caml_failwith_value(msg);
  }
  CAMLreturn(Val_unit);
}
