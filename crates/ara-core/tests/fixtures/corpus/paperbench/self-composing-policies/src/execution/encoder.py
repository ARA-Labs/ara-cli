"""
CNN Encoder for ALE Visual Observations.

Architecture (Appendix E.1):
  - 3 convolutional layers: 32, 64, 64 channels; filter sizes 8, 4, 3
  - 1 dense layer: output dimension 512

Used for SpaceInvaders and Freeway task sequences.
Each CompoNet module has its own encoder; new encoders are initialized
from the previous module's encoder weights (Appendix E.2).
"""

import torch
import torch.nn as nn
from typing import Tuple


class CNNEncoder(nn.Module):
    """
    3-layer CNN encoder for ALE image observations (210 × 160 RGB).
    Output: 512-dimensional feature vector h_s.

    Architecture from Appendix E.1 and Huang et al. (2022) / CleanRL.
    """

    def __init__(self, input_channels: int = 3, output_dim: int = 512):
        """
        Args:
            input_channels: Number of input image channels (3 for RGB)
            output_dim: Dimension of the output feature vector (d_enc = 512)
        """
        super().__init__()
        self.output_dim = output_dim

        self.conv = nn.Sequential(
            # Layer 1: 32 channels, filter size 8, stride 4
            nn.Conv2d(input_channels, 32, kernel_size=8, stride=4),
            nn.ReLU(),
            # Layer 2: 64 channels, filter size 4, stride 2
            nn.Conv2d(32, 64, kernel_size=4, stride=2),
            nn.ReLU(),
            # Layer 3: 64 channels, filter size 3, stride 1
            nn.Conv2d(64, 64, kernel_size=3, stride=1),
            nn.ReLU(),
        )

        # Compute conv output size for 210x160 input
        # After conv1 (k=8, s=4): floor((210-8)/4)+1=51, floor((160-8)/4)+1=39
        # After conv2 (k=4, s=2): floor((51-4)/2)+1=24, floor((39-4)/2)+1=18
        # After conv3 (k=3, s=1): floor((24-3)/1)+1=22, floor((18-3)/1)+1=16
        # Flattened: 64 * 22 * 16 = 22528
        self._conv_out_size = self._get_conv_out(input_channels)

        # Dense output layer
        self.fc = nn.Sequential(
            nn.Linear(self._conv_out_size, output_dim),
            nn.ReLU(),
        )

    def _get_conv_out(self, input_channels: int) -> int:
        """Compute conv output size by running a dummy forward pass."""
        dummy = torch.zeros(1, input_channels, 210, 160)
        out = self.conv(dummy)
        return int(out.view(1, -1).shape[1])

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        """
        Encode image observations to feature vectors.

        Args:
            x: Image tensor [batch, channels, height, width], values in [0, 255] or [0, 1]

        Returns:
            h_s: Feature vector [batch, 512]
        """
        # Normalize to [0, 1] if not already
        if x.dtype == torch.uint8:
            x = x.float() / 255.0
        conv_out = self.conv(x)  # [batch, 64, H', W']
        flat = conv_out.view(conv_out.size(0), -1)  # [batch, conv_out_size]
        h_s = self.fc(flat)  # [batch, 512]
        return h_s


def initialize_encoder_from_previous(
    new_encoder: CNNEncoder,
    prev_encoder: CNNEncoder,
) -> None:
    """
    Initialize a new module's encoder with the weights of the previous module's encoder.
    Used in CompoNet for ALE task sequences (Appendix E.2).

    Args:
        new_encoder: The newly created encoder (will be modified in-place)
        prev_encoder: The frozen encoder from the previous task's module
    """
    new_encoder.load_state_dict(prev_encoder.state_dict())
