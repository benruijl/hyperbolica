class IntegrationError(Exception):
    """Base class for errors raised by Hyperbolica."""


class InputError(IntegrationError):
    """Invalid expression, integration-variable schedule, or options."""


class DuplicateVariableError(InputError):
    variable: str


class AlgebraError(IntegrationError):
    """Failure of an exact algebra operation."""


class DivergentIntegralError(IntegrationError):
    boundary: str
    variable: str
    log_power: int
    power: int


class UnsupportedFeatureError(IntegrationError):
    """The requested integration feature is unavailable for this input."""


class ContextError(AlgebraError):
    variable: str



__version__: str
__symbolica_version__: str
__api_version__: int
