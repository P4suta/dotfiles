module valid_smoke {
  var n: int
  action init = n' = 0
  action step = n' = if (n == 0) 1 else 0
  val Inv = n.in(0.to(1))
}

module rejected_smoke {
  var n: int
  action init = n' = 0
  action step = n' = 2
  val Inv = n.in(0.to(1))
}
