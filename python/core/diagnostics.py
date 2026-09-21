"""Operational metadata only. Never log credentials or provider response bodies."""
import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path


def configure_logging(config_path):
    logger = logging.getLogger('core')
    logger.setLevel(logging.INFO)
    logger.propagate = False
    if logger.handlers:
        return
    try:
        handler = RotatingFileHandler(Path(config_path).with_name('diagnostics.log'),
                                      maxBytes=256 * 1024, backupCount=2, encoding='utf-8')
        handler.setFormatter(logging.Formatter('%(asctime)s %(name)s %(levelname)s %(message)s'))
        logger.addHandler(handler)
    except OSError:
        logger.addHandler(logging.NullHandler())
