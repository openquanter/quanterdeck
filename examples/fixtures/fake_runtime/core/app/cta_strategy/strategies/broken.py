# A module that will not import. The probe must report it as a finding
# and still return the strategies that did import.
raise RuntimeError("this strategy is deliberately broken")
