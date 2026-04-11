let NotEmpty = std.contract.from_predicate (fun arr =>
  std.array.length arr > 0
) in

let MaxVisibleFields = std.contract.from_predicate (fun fields =>
  let visible = std.array.filter (fun f => f.default_visible) fields in
  std.array.length visible >= 1 && std.array.length visible <= 6
) in

let AuthType = std.contract.from_predicate (fun x =>
  x == "none" || x == "bearer" || x == "basic" || x == "header"
) in

{
  Step = {
    name | String,
    upstream | String,
    tool | String,
    args | { _ : Dyn },
    depends_on | Array String | default = [],
    transform
      | {
        pick | Array String | optional,
        rename | { _ : String } | optional,
        filter | String | optional,
        truncate | { _ : Number } | optional,
      }
      | optional,
  },

  OutputField = {
    name | String,
    description | String,
    max_len | Number | optional,
    prefix | String | optional,
    default_visible | Bool | default = true,
  },

  Aggregate = {
    label | String,
    value | String,
  },

  NextStep = {
    command | String,
    description | String,
    when | String | optional,
  },

  Upstream = {
    url | String | optional,
    cmd | String | optional,
    args | Array String | default = [],
    auth
      | {
        type | AuthType,
        token | String | optional,
        headers | { _ : String } | optional,
      }
      | default = { type = "none" },
  },

  Tool = {
    description | String,
    detailed_help | String | optional,
    parameters
      | Array {
        name | String,
        type | String,
        description | String,
        required | Bool,
      }
      | default = [],
    steps | Array Step | NotEmpty,
    output_fields
      | Array OutputField
      | MaxVisibleFields,
    aggregates
      | Array Aggregate
      | NotEmpty,
    next_steps
      | Array NextStep
      | NotEmpty,
    empty_message | String,
    max_items | Number | default = 10,
  },

  Config = {
    upstreams | { _ : Upstream },
    tools | { _ : Tool },
  },
}
