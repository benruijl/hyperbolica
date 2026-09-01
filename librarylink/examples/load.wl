(* Load the Hyperbolica LibraryLink adapter and execute a small LR request.
   Set lib to the staged platform-specific library path first. *)

lib = Environment["HYPERBOLICA_LIBRARYLINK"];
If[!StringQ[lib] || !FileExistsQ[lib],
  Print["Set HYPERBOLICA_LIBRARYLINK to libhyperflint_librarylink.so/.dylib"];
  Exit[2]
];

hfVersion = LibraryFunctionLoad[lib, "hf_version", {}, "UTF8String"];
hfClearState = LibraryFunctionLoad[lib, "hf_clear_state", {}, Integer];
hfFindLROrders = LibraryFunctionLoad[
  lib, "hf_find_lr_orders", {"UTF8String"}, "UTF8String"
];
hfFactorTable = LibraryFunctionLoad[
  lib, "hf_factor_table", {"UTF8String"}, "UTF8String"
];
hfFindLROrdersScan = LibraryFunctionLoad[
  lib, "hf_find_lr_orders_scan", {"UTF8String"}, "UTF8String"
];
hfHyperFlintSym = LibraryFunctionLoad[
  lib, "hf_hyperflint_sym", {"UTF8String"}, "UTF8String"
];

Print["Hyperbolica LibraryLink version: ", hfVersion[]];
request = ExportString[
  <|"op" -> "find_lr_orders", "xvars" -> {"x"}, "polys" -> {"x+1"}|>,
  "JSON", "Compact" -> True
];
Print[ImportString[hfFindLROrders[request], "RawJSON"]];
Print["clear state: ", hfClearState[]];

