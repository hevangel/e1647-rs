# The IEEE 1647-2019 reflection catalogue (clause 32.1/32.2) — recognised names

This is a name catalogue, not a specification: it lists the clause 32.1
reflection type and method names and the 32.2 instance-iterator names by
their clause references, so user code referencing them is *recognised*
instead of rejected as an unknown name. No standard text is reproduced
here — every entry cites its 2019 clause only, and the signatures shown
are only the argument shapes the migration plan states
(`docs/e1647-rs_IEEE1647-2019_migration_plan.md`, Phase 11).

## Status in this stage (honest claims)

- **Recognised as stubs.** A call to one of the names below resolves to
  `E0419` "'name()' is recognised (clause X) but not implemented in this
  stage" at run time (the `UNIMPLEMENTED_2019` table in `src/interp.rs`;
  the type names in `RECOGNIZED_REFLECTION_TYPES` in `src/builtins.rs`).
  Arity is checked only where the plan states an argument shape; the
  other stubs accept any argument count and make no signature claim.
- **Implemented for real.** Only the clause 32.2 `instance_iterator`
  base type (T11.2) walks the instance tree — see the last section.
  The clause 32.3 table API is T9.3's surface, already implemented in
  Phase 9 (`src/tables.rs`, the `table_manager`/`table`/`table_row`
  values); it is listed here for completeness only.
- **No reflection model.** There are no `rf_*` values, no
  `rf_definition_element`s and no `rf_manager` singleton in this stage;
  every clause 32.1 entry below is a stub. A user-defined method of the
  same name always wins over the stub table.

## 32.1.2.3 — `rf_object` (base of `rf_struct` and `rf_interface`)

- `rf_object` (type)
- `rf_struct` (type)
- `rf_interface` (type)
- `is_contained_in()`
- `is_instance_of_me()`

## 32.1.2.5.2 — `rf_struct_member`

- `rf_struct_member` (type)
- `is_static()`

## 32.1.2.5.4 — `rf_range_generated_field`

- `rf_range_generated_field` (type)
- `was_generated()`
- `was_reduced()`
- `get_range()`
- `rgf_assign_value()`
- `rgf_assign_value_unsafe()`

## 32.1.2.5.8–32.1.2.5.10 — the `on`-method descriptors

- `rf_on_method` (type)
- `rf_on_event_method` (type)
- `rf_on_event_path_method` (type)
- `get_triggering_event()`
- `get_triggering_event_string()`

## 32.1.2.5.11 — `rf_event`

- `rf_event` (type)
- `on_method()`

## 32.1.2.5.13 — `rf_check`

- `rf_check` (type)
- `get_check_actions()`
- `get_relevant_check_actions()` (renamed in 2019)

## 32.1.2.6–32.1.2.7 — interfaces

- `rf_interface` members: `get_methods()`, `get_declared_methods()`,
  `get_method()`, `get_base_interfaces()`,
  `get_direct_sub_interfaces()`, `get_all_sub_interfaces()`,
  `get_implementors()`
- `rf_interface_member` (type): `get_declaring_interface()`
- `rf_interface_method` (type): `get_result_type()`,
  `get_parameters()`, `get_implementing_methods()`, `invoke()`,
  `invoke_unsafe()`

## 32.1.2.8.5 — `rf_like_struct`

- `get_interfaces()`

## 32.1.2.10 — scalars and numerics

- `rf_scalar` (type): `get_set_of_values()`,
  `get_full_set_of_values()` (moved from `rf_numeric` in 2019),
  `get_bit_size_modified(bits)`, `get_range_string()`
- `rf_numeric` (type): `is_longint()`
- `rf_custom_numeric` (type): `get_implementing_struct()`

## 32.1.2.13 — annotations

- `rf_annotation` (type)
- `rf_non_repeatable_annotation` (type)
- `rf_repeatable_annotation` (type)

(The clause 35.4 element-taking queries
`get_attached_annotation(element)` /
`get_attached_annotations_list(element)` are already recognised under
the T10.3 stubs — clause 32.1.3 cites the same queries on the
reflection side; the names are registered once, under clause 35.4.)

## 32.1.2.14 — the template family

- `rf_template_type` (type)
- `rf_template` (type)
- `rf_template_instance` (type)
- `rf_template_interface` (type), `rf_template_interface_instance` (type)
- `rf_template_numeric` (type), `rf_template_numeric_instance` (type)
- `rf_template_parameter` (type)
- `rf_type_template_parameter` (type)
- `rf_value_template_parameter` (type): `get_type()`
- `rf_template_instance_parameter` (type): `get_template_parameter()`
- `rf_type_template_instance_parameter` (type): `get_type()`
- `rf_value_template_instance_parameter` (type): `get_value()`

## 32.1.2.17 — `rf_manager` queries

- `rf_manager` (type)
- `get_annotation_type_by_name`
- `get_template_type_by_name`
- `get_template_by_name`
- `get_template_interface_by_name`
- `get_user_template_types`
- `get_user_templates`

## 32.1.3 — definition elements, layers, modules

- `rf_definition_element` (type): `get_source_lines_count()`
- `get_attached_annotations_list(element)` (see 32.1.2.13 above)
- `get_attached_annotation(element)` (see 32.1.2.13 above)
- `rf_struct_layer` (type): `get_added_interfaces()`,
  `get_exact_subtype()`
- `rf_template_layer` (type): `get_added_interfaces()`,
  `get_instance_layers()`
- `get_layers()` (on `rf_template`)
- `rf_template_instance_layer` (type): `get_template_layer()`
- `rf_module` (type): `get_actual_importer()`, `is_top_module()`,
  `get_full_file_name()`, `get_file_age()`

## 32.1.4 — value and manager utilities

- `rf_type` (type): `value_to_type(value, to_type)`
- `eval_exp_string(str)`
- `insert_list_element`, `insert_list_element_unsafe`
- `delete_list_element`, `delete_list_element_unsafe`
- `get_field_of_unit_instance(u)` (32.1.4.6)
- `get_list_indices_of_unit_instance(u)` (32.1.4.6)
- `get_list_index_of_unit_instance(u)` (32.1.4.6)

## 32.2 — the instance iterator (T11.2)

- `instance_iterator` — **implemented for real** (the base type only):
  `start(root)`, `start_all()`, `finish()`, `next()`,
  `get_current_instance()`, `get_current_depth()`,
  `get_current_path()`, `get_path_item_by_index(index)`,
  `get_current_path_item()`, `get_current_path_string()`.
  Depth-first walk over struct/unit instance fields from the root
  instance: lists flatten in index order, an already-visited object is
  not revisited, and the walk visits struct instances through instance
  fields only (no port traversal — the `with_ports` surface is not
  modelled). The current path renders `e_path`-style: `sys`, `sys.a`,
  `sys.a.b`, `sys.lst[0]`; the iteration root's path string is `sys`.
- `instance_path_item` (type) — recognised; constructing one is the
  `E0419` construction stub. The iterator's `get_current_path()` /
  `get_current_path_item()` / `get_path_item_by_index()` return real
  item values with `get_instance()`, `get_field()`,
  `get_list_index()`, `get_list_indices()`.
- `unit_instance_iterator` (type) — recognised; construction is the
  `E0419` stub. Only the base `instance_iterator` works.
- `unit_and_port_instance_iterator` (type) — recognised; construction
  is the `E0419` stub.
- `port_instance_iterator` (type) — recognised; construction is the
  `E0419` stub.
- The hooks `skip_field`, `skip_instance`, `with_ports` are recognised
  on an iterator value and report `E0419` (clause 32.2): they are
  user-extension hooks over the reflection model
  (`skip_field` takes an `rf_struct_member`), and extending a built-in
  type is not supported in this stage.

## 32.3 — the table API (T9.3, Phase 9)

Implemented in Phase 9 (see `docs/ROADMAP.md` and `src/tables.rs`):
the `table_manager` singleton, the `table` and `table_row` values and
their clause 32.3 methods, plus the clause 26 table pre-pass. Not part
of this catalogue's stub surface.
