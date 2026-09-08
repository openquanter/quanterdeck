"""A stand-in for the 1.x CtaTemplate, small enough to read.

Only the surface the probe touches: the two declaration lists and the
classmethod that turns them into defaults.
"""


class CtaTemplate:
    author = ""
    parameters: list = []
    variables: list = []

    @classmethod
    def get_class_parameters(cls) -> dict:
        return {name: getattr(cls, name) for name in cls.parameters}
