# Rubric Requirements — self-composing-policies
**Source**: PaperBench expert-authored reproduction rubric
**Total leaf requirements**: 275

## Method Implementation

### R01: The CNN has three convolutional layers with 32, 64 and 64 ch...
- **Rubric ID**: d92c7aa0-6a11-4141-a8f3-bc9b12bf4d37
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The CNN has three convolutional layers with 32, 64 and 64 channels and filter sizes of 8, 4 and 3 respectively.
- **Parent context**: A CNN has been implemented and is used to encode the state of both ALE environments to a low-dimensi...

### R02: The CNN has a final dense layer with an output dimension of...
- **Rubric ID**: 2eaeb892-c348-4acd-b603-30cbdd9f8ffc
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The CNN has a final dense layer with an output dimension of 512.
- **Parent context**: A CNN has been implemented and is used to encode the state of both ALE environments to a low-dimensi...

### R03: Logic to use a single CNN encoder per policy has been implem...
- **Rubric ID**: 2c2ef685-8326-4773-bb21-68f3e7a527b3
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to use a single CNN encoder per policy has been implemented, where the encoder for a new policy is initialized with the weights of the previous one, as described in Appendix E.2.
- **Parent context**: A convolutional neural network (CNN) has been implemented to encode the state of the ALE environment...

### R04: The output of the encoder is an input to both CompoNet (acto...
- **Rubric ID**: 1f4b310c-b311-42e0-a8f6-4e4e931cb6a5
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The output of the encoder is an input to both CompoNet (actor) as well as the value function (critic).
- **Parent context**: A convolutional neural network (CNN) has been implemented to encode the state of the ALE environment...

### R05: A matrix $\Phi^{k;s}$ of size $(k-1) \times |A|$ is defined...
- **Rubric ID**: dbbb11b3-c045-4e46-b2cc-cc1f652d84bf
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A matrix $\Phi^{k;s}$ of size $(k-1) \times |A|$ is defined which stores the output vectors of all preceding $k - 1$ modules for the current state, where $k$ is the number of tasks seen so far (including the current one) and $A$ is the action space.
- **Parent context**: The CompoNet architecture has been implemented, as described in Section 4.

### R06: Logic to compute $h_s \in \mathbb{R}^{d_{\text{enc}}}$ has b...
- **Rubric ID**: f0afda32-51d9-434d-be8e-5eb3ab5e72d3
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to compute $h_s \in \mathbb{R}^{d_{\text{enc}}}$ has been implemented, where $h_s$ is computed by the CNN encoder when encoding is needed (i.e., for the Arcade Learning Environment tasks) and $h_s = s$ when no encoder is needed (i.e., for the Meta-World tasks).
- **Parent context**: The CompoNet architecture has been implemented, as described in Section 4.

### R07: A parameter matrix $W_{out}^Q$ is defined with shape $d_{enc...
- **Rubric ID**: dda77699-d443-4fbf-90f4-59841597db57
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A parameter matrix $W_{out}^Q$ is defined with shape $d_{enc} \times d_{model}$ where $d_{enc}$ is the size of the encoded state vector and $d_{model}$ is the hidden size of the model.
- **Parent context**: The logic to compute the query vector for the output attention head has been implemented.

### R08: The query vector is computed via $q = h_s W_{out}^Q$ where $...
- **Rubric ID**: 7ce56b99-e970-469c-8031-09d4697acf3e
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The query vector is computed via $q = h_s W_{out}^Q$ where $h_s$ is the encoded state vector.
- **Parent context**: The logic to compute the query vector for the output attention head has been implemented.

### R09: A parameter matrix $W_{out}^K$ is defined with shape $|A| \t...
- **Rubric ID**: 060b1284-6e1b-4692-bd53-64e043c07fd3
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A parameter matrix $W_{out}^K$ is defined with shape $|A| \times d_{model}$ where $A$ is the action space and $d_{model}$ is the hidden size of the model.
- **Parent context**: The logic to compute the key vector for the output attention head has been implemented.

### R10: A cosine positional embedding matrix $E_{out}$ of size $(k-1...
- **Rubric ID**: cde1da9b-47ce-4d8d-80c4-ab2a0cb678b0
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A cosine positional embedding matrix $E_{out}$ of size $(k-1) \times |A|$ is defined.
- **Parent context**: The logic to compute the key vector for the output attention head has been implemented.

### R11: The key vector is computed via $(\Phi^{k;s} + E_{out}) W_{ou...
- **Rubric ID**: 0b8be057-bf1b-44fe-9d9f-0f4ca535d12b
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The key vector is computed via $(\Phi^{k;s} + E_{out}) W_{out}^Q$.
- **Parent context**: The logic to compute the key vector for the output attention head has been implemented.

### R12: The logic to compute the value vector for the output attenti...
- **Rubric ID**: 48b73614-82f1-4d32-b2af-427ab51924b9
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The logic to compute the value vector for the output attention head has been implemented, which is defined as $\Phi^{k;s}$ (the matrix of outputs from all previous modules).
- **Parent context**: The output attention head has been implemented, as described in Section 4.2 -- "Output Attention Hea...

### R13: The output vector $v$ for the output attention head is compu...
- **Rubric ID**: 76fa75da-2e3e-4260-9e2f-09ad3e0e9259
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The output vector $v$ for the output attention head is computed via $\text { Attention }(\mathbf{q}, K, V)=\text{softmax}\left(\frac{q K^T}{\sqrt{d_{\text {model }}}}\right) V$.
- **Parent context**: The output attention head has been implemented, as described in Section 4.2 -- "Output Attention Hea...

### R14: The only learnable parameters for the output attention head...
- **Rubric ID**: bd7db785-3a65-42bc-bbf8-f3818a19700a
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The only learnable parameters for the output attention head are $W_{out}^Q$ and $W_{out}^K$.
- **Parent context**: The output attention head has been implemented, as described in Section 4.2 -- "Output Attention Hea...

### R15: A matrix $P$ is defined as the row-wise concatenation of the...
- **Rubric ID**: d68e4314-52f6-4e6d-9aa3-a85f0bd9706d
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A matrix $P$ is defined as the row-wise concatenation of the output of the previous block (i.e., the output attention head) and $\Phi^{k;s}$ (the matrix of outputs from all previous modules).
- **Parent context**: The input attention head has been implemented, as described in Section 4.2 -- "Input Attention Head"...

### R16: A parameter matrix $W_{in}^Q$ is defined with shape $d_{enc}...
- **Rubric ID**: cf3c7ec9-b96a-4a89-a688-b7ba1bb56148
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A parameter matrix $W_{in}^Q$ is defined with shape $d_{enc} \times d_{model}$ where $d_{enc}$ is the size of the encoded state vector and $d_{model}$ is the hidden size of the model.
- **Parent context**: The logic to compute the query vector for the input attention head has been implemented.

### R17: The query vector is computed via $q = h_s W_{in}^Q$ where $h...
- **Rubric ID**: ef07a797-146a-479d-9b9f-cfb2bef36599
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The query vector is computed via $q = h_s W_{in}^Q$ where $h_s$ is the encoded state vector.
- **Parent context**: The logic to compute the query vector for the input attention head has been implemented.

### R18: A parameter matrix $W_{in}^K$ is defined with shape $|A| \ti...
- **Rubric ID**: 83e80a16-d7bb-4e57-809b-be04bccacd9a
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A parameter matrix $W_{in}^K$ is defined with shape $|A| \times d_{model}$ where $A$ is the action space and $d_{model}$ is the hidden size of the model.
- **Parent context**: The logic to compute the key vector for the input attention head has been implemented.

### R19: A cosine positional embedding matrix $E_{in}$ of the same si...
- **Rubric ID**: b40e5cba-8422-4aa1-a638-44e515d99f27
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A cosine positional embedding matrix $E_{in}$ of the same size as $P$ is defined.
- **Parent context**: The logic to compute the key vector for the input attention head has been implemented.

### R20: The key vector is computed via $(P + E_{in}) W_{in}^K$.
- **Rubric ID**: a75ab485-bbc2-4932-ab22-6be8c55498e3
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The key vector is computed via $(P + E_{in}) W_{in}^K$.
- **Parent context**: The logic to compute the key vector for the input attention head has been implemented.

### R21: A parameter matrix $W_{in}^V$ is defined with shape $|A| \ti...
- **Rubric ID**: eeb5aa73-4d41-49f5-ad43-05cfc3246084
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A parameter matrix $W_{in}^V$ is defined with shape $|A| \times d_{model}$ where $A$ is the action space and $d_{model}$ is the hidden size of the model.
- **Parent context**: The logic to compute the value vector for the input attention head has been implemented.

### R22: The value vector is computed via $P W_{in}^V$.
- **Rubric ID**: 467eb0c9-00c4-4aea-b508-4a7c2669f31b
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The value vector is computed via $P W_{in}^V$.
- **Parent context**: The logic to compute the value vector for the input attention head has been implemented.

### R23: The output vector for the input attention head is computed v...
- **Rubric ID**: 2c041f24-2aed-4128-ba54-b27407f67db8
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The output vector for the input attention head is computed via $\text { Attention }(\mathbf{q}, K, V)=\text{softmax}\left(\frac{q K^T}{\sqrt{d_{\text {model }}}}\right) V$.
- **Parent context**: The input attention head has been implemented, as described in Section 4.2 -- "Input Attention Head"...

### R24: The only learnable parameters for the input attention head a...
- **Rubric ID**: a6657076-1593-46c2-bac7-c83f69d44797
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The only learnable parameters for the input attention head are $W_{in}^Q$, $W_{in}^K$ and $W_{in}^V$.
- **Parent context**: The input attention head has been implemented, as described in Section 4.2 -- "Input Attention Head"...

### R25: The encoded state vector $h_s$ and the output of the input a...
- **Rubric ID**: d861366a-340a-4fc8-9c02-ce0ad39c844e
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The encoded state vector $h_s$ and the output of the input attention head module are concatenated column-wise, creating a vector of size $d_{enc} + d_{model}$.
- **Parent context**: The internal policy has been implemented, as described in Section 4.2 -- "Internal Policy"

### R26: A multi-layer feed-forward block is implemented which maps a...
- **Rubric ID**: 0d54e498-4838-450f-a17b-59ac2155855b
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A multi-layer feed-forward block is implemented which maps a vector of length $d_{enc} + d_{model}$ to one of length $|A|$, where |A| is the dimensionality of the action space.
- **Parent context**: The internal policy has been implemented, as described in Section 4.2 -- "Internal Policy"

### R27: The only learnable parameters for the internal policy are th...
- **Rubric ID**: b2387837-a2b3-41d0-8866-0516d7cb1067
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The only learnable parameters for the internal policy are those in the multi-layer feed-forward block.
- **Parent context**: The internal policy has been implemented, as described in Section 4.2 -- "Internal Policy"

### R28: An $|A|$-dimension vector is computed by adding the output o...
- **Rubric ID**: 5eb4989c-fa80-497f-935d-b27397ce63b8
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: An $|A|$-dimension vector is computed by adding the output of the internal policy module, a vector of size $|A|$, to the output from the output attention head, the vector previously denoted as $v$.
- **Parent context**: The CompoNet architecture has been implemented, as described in Section 4.

### R29: Logic to normalize the summed vectors has been implemented f...
- **Rubric ID**: 74545ee1-9594-4d2f-89f0-374f8f48c745
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to normalize the summed vectors has been implemented for continuous action spaces.
- **Parent context**: The CompoNet architecture has been implemented, as described in Section 4.

### R30: The logic to generate a sequence of 20 tasks (i.e., a sequen...
- **Rubric ID**: d1f984f0-9a75-451d-9fe2-a20e1a445909
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The logic to generate a sequence of 20 tasks (i.e., a sequence of all 10 Meta-World environments repeated repeated twice) has been implemented, as described in Section 5.2.
- **Parent context**: All Meta-World environments described in Section 5.2 and Appendix D are accessible in code using the...

### R31: The logic to generate a sequence of 17 tasks (i.e., a sequen...
- **Rubric ID**: f7c172f9-372e-4d43-8fa0-64d692abf65c
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The logic to generate a sequence of 17 tasks (i.e., a sequence of all 10 playing modes of `ALE/SpaceInvaders-v5` followed by all 7 playing modes of `ALE/Freeway-v5`) has been implemented, as described in Section 5.2.
- **Parent context**: All ALE environments are accessible in code with the correct observation and action spaces, as descr...

### R32: The SAC implementation from Huang et al. (2022) has been ada...
- **Rubric ID**: b57de4fc-b531-48b4-b4ab-c685126baf65
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The SAC implementation from Huang et al. (2022) has been adapted to use the CompoNet as an agent.
- **Parent context**: The SAC algorithm has been implemented by adapting the implementation from Huang et al. (2022), as d...

### R33: The SAC implementation from Huang et al. (2022) has been fur...
- **Rubric ID**: d99f41a7-6157-4ba6-9b5c-52f5ac11c5d3
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The SAC implementation from Huang et al. (2022) has been further adapted to be compatible with all 10 Meta-World environments used in the paper. TODO: Clarify the specific modifications made with the author.
- **Parent context**: The SAC algorithm has been implemented by adapting the implementation from Huang et al. (2022), as d...

### R34: The critic network has been implemented as a two-layer MLP f...
- **Rubric ID**: 9fe63a30-dd3d-4889-a8c3-8dd04a57c8f1
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The critic network has been implemented as a two-layer MLP followed by two separate output heads of a single layer corresponding to the mean and logarithm of the standard deviation of a normal distribution.
- **Parent context**: The SAC algorithm has been implemented by adapting the implementation from Huang et al. (2022), as d...

### R35: In the SAC implementation, the activation function is set to...
- **Rubric ID**: b3569d2d-1799-4842-8bc8-a9edbc5d2043
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the SAC implementation, the activation function is set to ReLU.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R36: In the SAC implementation,  the hidden dimension ($d_{\text{...
- **Rubric ID**: 83b0ab19-62d1-4eff-982d-53a73493a824
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the SAC implementation,  the hidden dimension ($d_{\text{model}}$) is set to 256,
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R37: In the SAC implementation, there are 3 layers in the target...
- **Rubric ID**: 1df005b8-7e5f-4161-bd38-bca97c72a061
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the SAC implementation, there are 3 layers in the target network.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R38: In the SAC implementation, there are 3 layers in the critic...
- **Rubric ID**: 98c55d23-669d-4f17-9b96-a47318faadd8
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the SAC implementation, there are 3 layers in the critic network.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R39: The PPO implementation from Huang et al. (2022) has been ada...
- **Rubric ID**: 90f271f7-eba5-4a1e-a105-2212eb5f351f
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The PPO implementation from Huang et al. (2022) has been adapted to use the CompoNet as an agent.
- **Parent context**: The PPO algorithm has been implemented by adapting the implementation from Huang et al. (2022), as d...

### R40: Two single-layer output heads are used to generate the logit...
- **Rubric ID**: 93d94ac7-8757-47cd-88da-cbedb56ca63b
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Two single-layer output heads are used to generate the logits of the categorical distribution over the action space (actor) and to compute the value function (critic).
- **Parent context**: The PPO algorithm has been implemented by adapting the implementation from Huang et al. (2022), as d...

### R41: In the PPO implementation, the activation function has been...
- **Rubric ID**: 529ab85f-d8a7-40f3-a3a9-b1b3d34d6cc6
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the PPO implementation, the activation function has been set to ReLU.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R42: In the PPO implementation, the value loss is clipped.
- **Rubric ID**: 23e124f9-b156-4da0-80ba-b6fdaea6e4bc
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the PPO implementation, the value loss is clipped.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R43: In the PPO implementation, advantage estimates are normalize...
- **Rubric ID**: a06953ec-2fd7-4806-96c0-831260cd57ed
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the PPO implementation, advantage estimates are normalized.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R44: Logic to reset the critic network at the beginning of each t...
- **Rubric ID**: f94431f2-3d7d-482c-bf47-119f41e084d1
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to reset the critic network at the beginning of each task has been implemented.
- **Parent context**: The critic network has been implemented and configured correctly.

### R45: In the case of the Meta-World environments, the layer maps a...
- **Rubric ID**: e78a823d-9418-4df4-ab5a-af008c73b952
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the case of the Meta-World environments, the layer maps a 39-dimensional state vector down to a single scalar.
- **Parent context**: The value function (critic) has been implemented as a single fully connected layer.

### R46: In the case of the ALE environments, the layer maps an image...
- **Rubric ID**: 9db10e5b-a00b-4a6e-b011-fdd7d1c56e6d
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: In the case of the ALE environments, the layer maps an image of size 210x160 down to a single scalar.
- **Parent context**: The value function (critic) has been implemented as a single fully connected layer.

### R47: The baseline method has been implemented for the Meta-World...
- **Rubric ID**: 97ddc4c5-192e-4faf-9fb0-628b63411a87
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The baseline method has been implemented for the Meta-World environments, which is a randomly initialized MLP that maps a 39-dimensional state vector to two separate heads: one outputting the mean of a Gaussian distribution for each of the 4 actions and another outputting the log standard deviations.
- **Parent context**: The baseline method has been implemented for the Meta-World and ALE environments, as described in Se...

### R48: The baseline method has been implemented for the ALE environ...
- **Rubric ID**: 0345c389-bef7-4968-98bd-7c76912dffa5
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The baseline method has been implemented for the ALE environments, which is a randomly intialized MLP mapping a 512-dimensional state vector to a 6-dimensional action space. The network outputs logits for a categorical distribution over these 6 actions.
- **Parent context**: The baseline method has been implemented for the Meta-World and ALE environments, as described in Se...

### R49: The FT-1 method has been implemented for the Meta-World envi...
- **Rubric ID**: 63030bd7-0708-45a6-b40d-c8ab3f46b88f
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The FT-1 method has been implemented for the Meta-World environments, which is a randomly initialized MLP that maps a 39-dimensional state vector to two separate heads: one outputting the mean of a Gaussian distribution for each of the 4 actions and another outputting the log standard deviations.
- **Parent context**: FT-1 has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R50: The FT-1 method has been implemented for the ALE environment...
- **Rubric ID**: 73ae5d74-de2e-4427-82af-1b1d7f5217e8
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The FT-1 method has been implemented for the ALE environments, which is a randomly intialized MLP mapping a 512-dimensional state vector to a 6-dimensional action space. The network outputs logits for a categorical distribution over these 6 actions.
- **Parent context**: FT-1 has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R51: The FT-1 method has been implemented so that it continuously...
- **Rubric ID**: 798a234f-1e95-4d27-8d74-4b1eaee220a1
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The FT-1 method has been implemented so that it continuously fine-tune a single NN across all tasks.
- **Parent context**: FT-1 has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R52: The FT-N method has been implemented for the Meta-World envi...
- **Rubric ID**: de8d83b9-639e-4c95-b473-6a8dea90ee08
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The FT-N method has been implemented for the Meta-World environments, which is a randomly initialized MLP that maps a 39-dimensional state vector to two separate heads: one outputting the mean of a Gaussian distribution for each of the 4 actions and another outputting the log standard deviations.
- **Parent context**: FT-N has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R53: The FT-N method has been implemented for the ALE environment...
- **Rubric ID**: 4fd52a4e-a799-428b-8910-cd799727240e
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The FT-N method has been implemented for the ALE environments, which is a randomly intialized MLP mapping a 512-dimensional state vector to a 6-dimensional action space. The network outputs logits for a categorical distribution over these 6 actions.
- **Parent context**: FT-N has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R54: The logic to re-initialize the output heads at the beginning...
- **Rubric ID**: bce385fe-76ec-46e5-a00a-dc45d2eb984e
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The logic to re-initialize the output heads at the beginning of each task has been implemented.
- **Parent context**: FT-N has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R55: Logic to instantiate a new network (with random initial para...
- **Rubric ID**: b7dccfee-06e4-4f55-959a-05ce7545efa7
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to instantiate a new network (with random initial parameters) every time the task changes has been implemented.
- **Parent context**: ProgressiveNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and ...

### R56: Logic to add lateral connections (TODO: be more specific abo...
- **Rubric ID**: b363705b-edc3-4a6c-8d74-1883abb02bb6
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to add lateral connections (TODO: be more specific about what a lateral connection is after asking author) between the current network and the ones learned in previous tasks when a new network is added has been implemented.
- **Parent context**: ProgressiveNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and ...

### R57: Logic to freeze the parameters of the neural networks traine...
- **Rubric ID**: 9ff4ad97-4f13-4af4-a1c0-e1dd19981e83
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to freeze the parameters of the neural networks trained on previous tasks has been implemented.
- **Parent context**: ProgressiveNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and ...

### R58: Logic to save the parameters of the last neural network trai...
- **Rubric ID**: 21b4d6bc-062e-43ab-86a7-f3c98daf996e
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to save the parameters of the last neural network trained when the task changes has been implemented.
- **Parent context**: ProgressiveNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and ...

### R59: The input of every layer of the new network includes the out...
- **Rubric ID**: a39fe09d-59d5-4ad8-876b-5e2c34006f9e
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: The input of every layer of the new network includes the outputs of all the layers from the networks learned in previous tasks.
- **Parent context**: ProgressiveNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and ...

### R60: Logic to prune the trained network after each task has been...
- **Rubric ID**: 1cbbd1d1-e58f-4b33-907d-2d59fd9e1ca3
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to prune the trained network after each task has been implemented, selecting the weights that are most relevant for the current task, has been implemented.
- **Parent context**: PackNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and replace...

### R61: Logic to retrain the pruned network for the current task has...
- **Rubric ID**: 60c403cd-23cd-41d6-9a7b-a2713a5a33a5
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to retrain the pruned network for the current task has been implemented has been implemented.
- **Parent context**: PackNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and replace...

### R62: Logic to freeze the selected parameters of the pruned networ...
- **Rubric ID**: b4f5acb5-4faa-4346-bd80-cbc3f65bc5d7
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to freeze the selected parameters of the pruned network for the rest of the future tasks has been implemented.
- **Parent context**: PackNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and replace...

### R63: Logic to decide how many parameters can be pruned and stored...
- **Rubric ID**: 57420140-abf8-45c7-ad1e-457a5f211253
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Logic to decide how many parameters can be pruned and stored per task has been implemented, as described in Appendix E.2.
- **Parent context**: PackNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and replace...

### R64: TODO: Logic to create a sequence of 300 tasks has been imple...
- **Rubric ID**: 82e6b6ac-73f7-4c6c-963f-7bf20db11ff2
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: TODO: Logic to create a sequence of 300 tasks has been implemented using X (e.g. Meta-World or ALE or both).
- **Parent context**: The inference time (left) sub-figure has been reproduced.

### R65: TODO: Logic to create a sequence of 300 tasks has been imple...
- **Rubric ID**: 8b74b662-4ec5-4b9c-a9ab-9f3276d75913
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: TODO: Logic to create a sequence of 300 tasks has been implemented using X (e.g. Meta-World or ALE or both).
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R66: A single informative policy (i.e., one that provides relevan...
- **Rubric ID**: b777444f-44ac-45fd-abf4-25162b3911fb
- **Category**: Code Execution / Method Implementation
- **Weight**: 1
- **Requirement**: A single informative policy (i.e., one that provides relevant knowledge for solving a future task) has been pre-trained on the 5th task (i.e., the 4th playing mode) of SpaceInvaders using the SAC algorithm and the hyperparameters in Table E.1 (TODO: Confirm with author), with its weights saved for later reuse.
- **Parent context**: Figures 4 (a) - (d) have been reproduced. CompoNet is trained on the 5th task (i.e., the 4th playing...

### R67: Four non-informative policies have been defined and implemen...
- **Rubric ID**: feec8fca-32d5-47d1-b5f5-6c9e0f5338b0
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Four non-informative policies have been defined and implemented to act as random policies i.e., they each sample an action from a uniform Dirichlet distribution.
- **Parent context**: Figures 4 (a) - (d) have been reproduced. CompoNet is trained on the 5th task (i.e., the 4th playing...

### R68: A new CompoNet module has been instantiated for the 5th task...
- **Rubric ID**: 5f0ec7d2-cc32-4378-bea1-56b1ff9883c9
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A new CompoNet module has been instantiated for the 5th task of SpaceInvaders, referencing the four non-informative modules and the single informative module as its (frozen) predecessors, ensuring the parameters of all previous modules are frozen.
- **Parent context**: CompoNet was trained on the 5th task (i.e., the 4th playing mode) of SpaceInvaders for 1M timesteps ...

### R69: The output attention head of CompoNet has computed a probabi...
- **Rubric ID**: adc3939d-e953-4805-849c-193864b8ba0f
- **Category**: Code Execution / Method Implementation
- **Weight**: 1
- **Requirement**: The output attention head of CompoNet has computed a probability distribution over actions at each time step for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R70: The final output of CompoNet has computed a probability dist...
- **Rubric ID**: eda7a58b-1475-4a8b-9d8c-931a46d95324
- **Category**: Code Execution / Method Implementation
- **Weight**: 1
- **Requirement**: The final output of CompoNet has computed a probability distribution over actions at each time step for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R71: Five non-informative policies have been defined and implemen...
- **Rubric ID**: a16cd6bb-5d47-45b0-995f-81f3485997bd
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: Five non-informative policies have been defined and implemented to act as random policies i.e., they each sample an action from a uniform Dirichlet distribution.
- **Parent context**: Figures 4 (e) - (h) have been reproduced. CompoNet is trained on the 6th task (i.e., the 5th playing...

### R72: A new CompoNet module has been instantiated for the 6th task...
- **Rubric ID**: a377e3c6-61c8-405f-8459-699347bef89a
- **Category**: Code Development / Method Implementation
- **Weight**: 1
- **Requirement**: A new CompoNet module has been instantiated for the 6th task (i.e., the 5th playing mode) of SpaceInvaders, referencing the 5 non-informative policies as its predecessors.
- **Parent context**: CompoNet was trained on the 6th task (i.e., the 5th playing mode) of SpaceInvaders for 1M timesteps ...

### R73: The output attention head of CompoNet has computed a probabi...
- **Rubric ID**: b68b1fa6-fc9e-4f4e-9b14-edddc40c6eb4
- **Category**: Code Execution / Method Implementation
- **Weight**: 1
- **Requirement**: The output attention head of CompoNet has computed a probability distribution over actions at each time step for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R74: The output of the internal policy of CompoNet has computed a...
- **Rubric ID**: 82686c84-e70e-4c14-a56e-353091dab34a
- **Category**: Code Execution / Method Implementation
- **Weight**: 1
- **Requirement**: The output of the internal policy of CompoNet has computed a probability distribution over actions at each time step for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R75: The final output of CompoNet has computed a probability dist...
- **Rubric ID**: f2ea520b-b055-4f37-99e5-ed10f6164317
- **Category**: Code Execution / Method Implementation
- **Weight**: 1
- **Requirement**: The final output of CompoNet has computed a probability distribution over actions at each time step for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

## Evaluation, Metrics & Benchmarking

### R76: The success rate for task $i$ at timestep $t$, $p_i(t)$, is...
- **Rubric ID**: 37d0398b-02ff-45cb-a378-9229584fcfb3
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rate for task $i$ at timestep $t$, $p_i(t)$, is available for all timesteps and is taken as an input, which corresponds to the success rate in task $i$ at time $t$.
- **Parent context**: The average performance metric has been computed, as described in Section 5.1 -- "Average Performanc...

### R77: The average performance at timestep $t$ is computed as $P(t)...
- **Rubric ID**: 0f95bb2e-1c2f-4394-a367-3d61dc8de2c4
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The average performance at timestep $t$ is computed as $P(t) = \frac{1}{N} \sum_{i=1}^N p_i(t)$
- **Parent context**: The average performance metric has been computed, as described in Section 5.1 -- "Average Performanc...

### R78: The success rate for task $i$ at timestep $t$, $p_i(t)$, is...
- **Rubric ID**: 09cd146f-2424-4bb1-b844-a3268856492d
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rate for task $i$ at timestep $t$, $p_i(t)$, is available for all timesteps and is taken as an input, which corresponds to the success rate in task $i$ at time $t$.
- **Parent context**: The forward transfer metric has been implemented, as described in Section 5.1 -- "Forward Transfer".

### R79: The baseline (i.e., training a randomly initialized NN for e...
- **Rubric ID**: 3f61bfcd-04a9-4b08-a4c9-614b901315c0
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The baseline (i.e., training a randomly initialized NN for each task) success rate for each task, $p_i^b(t)$, is taken as an input, which corresponds to the success rate of the baseline method in task $i$ at time $t$.
- **Parent context**: The forward transfer metric has been implemented, as described in Section 5.1 -- "Forward Transfer".

### R80: The area under the success curve ($\mathrm{AUC}_i=\frac{1}{\...
- **Rubric ID**: 07c21e51-90f4-43c4-986f-4d572829ffef
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The area under the success curve ($\mathrm{AUC}_i=\frac{1}{\Delta} \int_{(i-1) \cdot \Delta}^{i \cdot \Delta} p_i(t) \mathrm{d} t$) for each task is computed, as described in Equation (2) of Section 5.1.
- **Parent context**: The forward transfer metric has been implemented, as described in Section 5.1 -- "Forward Transfer".

### R81: The area under the baseline success curve ($\mathrm{AUC}_i^b...
- **Rubric ID**: dbe45d3b-284d-40d2-9422-c3f4c75b1540
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The area under the baseline success curve ($\mathrm{AUC}_i^b=\frac{1}{\Delta} \int_0^{\Delta} p_i^b(t) \mathrm{d} t$) for each task is computed, as described in Equation (2) of Section 5.1.
- **Parent context**: The forward transfer metric has been implemented, as described in Section 5.1 -- "Forward Transfer".

### R82: The forward transfer metric $\mathrm{FTr}_i=\frac{\mathrm{AU...
- **Rubric ID**: a16438be-02e0-49b4-b3d4-bbbe6f53b6af
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The forward transfer metric $\mathrm{FTr}_i=\frac{\mathrm{AUC}_i-\mathrm{AUC}_i^b}{1-\mathrm{AUC}_i^b}$ has is computed, as described in Equation (2) of Section 5.1.
- **Parent context**: The forward transfer metric has been implemented, as described in Section 5.1 -- "Forward Transfer".

### R83: The forward transfers, $\text{FTr}(j,i)$, obtained by traini...
- **Rubric ID**: 05714e8d-caa5-4f88-b903-8595f1692b34
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The forward transfers, $\text{FTr}(j,i)$, obtained by training a model from scratch on the $j$-th task and fine-tuning it on the $i$-th task are taken as inputs for all $i$ and $j$ such that $1 \leq j < i \leq N$.
- **Parent context**: The reference forward transfer metric has been implemented, as described in Section 5.1 -- "Referenc...

### R84: Code for computing the reference forward transfer ($\mathrm{...
- **Rubric ID**: c76c362b-3f12-4420-a651-49cce1f7aac4
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Code for computing the reference forward transfer ($\mathrm{RT}=\frac{1}{N} \sum_{i=2}^N \max _{j<i} \mathrm{FTr}(j, i)$) has been implemented, as described in Equation (3) of Section 5.1.
- **Parent context**: The reference forward transfer metric has been implemented, as described in Section 5.1 -- "Referenc...

### R85: CompoNet's inference time has been measured for an increasin...
- **Rubric ID**: b52da4ce-9be2-4046-afdf-061da3fcf5bb
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: CompoNet's inference time has been measured for an increasing number of tasks (from 1 up to 300, inclusive) whilst being trained on the Meta-World environments (TODO: Confirm this with the author). The measurements are conducted with the hyperparameters from Table E.1 and the caption of Figure 3 ([TODO: Confirm this with the author, remove once confirmed] giving precedence to the caption in the case of conflict), as described in Appendix C.2, with the results stored in a suitable data structure.
- **Parent context**: The inference time (left) sub-figure has been reproduced.

### R86: ProgressiveNet's inference time has been measured for an inc...
- **Rubric ID**: 043beaa5-0930-4535-866b-0cc8337649d4
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: ProgressiveNet's inference time has been measured for an increasing number of tasks (from 1 up to 300, inclusive) whilst being trained on the Meta-World environments (TODO: Confirm this with the author). The measurements are conducted with the hyperparameters from Table E.1 and the caption of Figure 3 ([TODO: Confirm this with the author, remove once confirmed] giving precedence to the caption in the case of conflict), as described in Appendix C.2, with the results stored in a suitable data structure.
- **Parent context**: The inference time (left) sub-figure has been reproduced.

### R87: CompoNet's total and trainable parameter counts have been me...
- **Rubric ID**: 1f60a8c9-47d0-4b12-adf6-3cff221ba3df
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: CompoNet's total and trainable parameter counts have been measured for an increasing number of tasks (from 1 up to 300, inclusive) whilst being trained on the Meta-World environments (TODO: Confirm this with the author). The measurements are conducted with the hyperparameters from Table E.1 and the caption of Figure 3 ([TODO: Confirm this with the author, remove once confirmed] giving precedence to the caption in the case of conflict), as described in Appendix C.2, with the results stored in a suitable data structure.
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R88: ProgressiveNet's total and trainable parameter counts have b...
- **Rubric ID**: 3a78e866-1122-4dd4-a6e1-c2f65fbed097
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: ProgressiveNet's total and trainable parameter counts have been measured for an increasing number of tasks (from 1 up to 300, inclusive) whilst being trained on the Meta-World environments (TODO: Confirm this with the author). The measurements are conducted with the hyperparameters from Table E.1 and the caption of Figure 3 ([TODO: Confirm this with the author, remove once confirmed] giving precedence to the caption in the case of conflict), as described in Appendix C.2, with the results stored in a suitable data structure.
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R89: The success rates for each task and seed has been recorded.
- **Rubric ID**: cae11072-9e07-4f63-b0cf-948d7ba2da8c
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The FT-N method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R90: The success rates for each task and seed has been recorded.
- **Rubric ID**: a7c5323e-0aac-4977-898c-34246b2bf3a9
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The baseline method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R91: The success rates for each task and seed has been recorded.
- **Rubric ID**: c2527a83-2d29-4f46-959f-4f3387f73706
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The FT-N method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R92: The success rates for each task and seed has been recorded.
- **Rubric ID**: 6b42c4a3-b154-4081-90fa-549c842d6e0b
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The baseline method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R93: The success rates for each task and seed has been recorded.
- **Rubric ID**: f7d07e22-288d-4ce1-9b09-959049f24191
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The FT-N method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R94: The success rates for each task and seed has been recorded.
- **Rubric ID**: c2a01219-92e0-4401-8a54-2db1d173c8c6
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The PackNet method has been trained on the Freeway sequence of tasks, and the success rates have bee...

### R95: The success rates for each task and seed has been recorded.
- **Rubric ID**: 26a9496e-d671-475a-9267-e57c9cfc978f
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The CompoNet method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R96: The average performance metric has been computed for each me...
- **Rubric ID**: 15067927-78c4-4c88-b141-e55c48d21df4
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The average performance metric has been computed for each method and seed (i.e., 50 metrics in total).
- **Parent context**: All methods have been trained on the three task sequences (Meta-World, SpaceInvaders, Freeway) for 1...

### R97: The forward transfer metric has been computed for each metho...
- **Rubric ID**: a8a243b1-007f-4f35-a6e9-31e41fb88c6b
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The forward transfer metric has been computed for each method and seed (i.e., 50 metrics in total).
- **Parent context**: All methods have been trained on the three task sequences (Meta-World, SpaceInvaders, Freeway) for 1...

### R98: The mean and standard deviation of the average performance a...
- **Rubric ID**: ddd24b5c-d32b-4b21-b7f0-e6db01fbfc05
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean and standard deviation of the average performance and forward transfer metrics have been computed for every method and every task sequence across all 10 seeds using the saved results.
- **Parent context**: Table 1 has been reproduced.

### R99: The mean average performance metrics show that CompoNet pref...
- **Rubric ID**: 319edc0d-4cf8-4a71-933c-49f4155a2588
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean average performance metrics show that CompoNet preforms at least as well as all other methods (higher is better) across all three task sequences.
- **Parent context**: Table 1 has been reproduced.

### R100: The mean forward transfer metrics show that CompoNet preform...
- **Rubric ID**: 0d79cc8d-c8b2-4e97-a910-95090cd2338b
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean forward transfer metrics show that CompoNet preforms at least as well as all other methods (higher is better) across all three task sequences.
- **Parent context**: Table 1 has been reproduced.

### R101: Code to compute and save the episodic returns at regular int...
- **Rubric ID**: 96bf2ae6-8194-4535-9b25-7d078434ddb2
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Code to compute and save the episodic returns at regular intervals (at least every 10k timesteps) for each seed has been implemented.
- **Parent context**: Figure 4 has been reproduced.

### R102: Code to compute whether the action with the highest probabil...
- **Rubric ID**: 6995dcdb-e1fb-443b-a5cf-f9a51e1d04b0
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Code to compute whether the action with the highest probability from distribution matches the action with the highest probability from another distribution has been implemented.
- **Parent context**: Figure 4 has been reproduced.

### R103: Code to compute and save the matching rate between two distr...
- **Rubric ID**: 985db9fe-0c97-4b2e-8025-a27d5f446ca3
- **Category**: Code Development / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Code to compute and save the matching rate between two distributions at least every 10k timesteps for each seed has been implemented.
- **Parent context**: Figure 4 has been reproduced.

### R104: The episodic returns have been computed at regular intervals...
- **Rubric ID**: d1993d1e-d8c6-4ec0-bd8d-74d21fef01b5
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The episodic returns have been computed at regular intervals (at least every 10k timesteps) and saved during training of CompoNet for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (a) have been computed and saved.

### R105: The output of the internal policy of CompoNet has computed a...
- **Rubric ID**: dea1e542-e600-4629-b27e-d47dc20a99be
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The output of the internal policy of CompoNet has computed a probability distribution over actions at each time step for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R106: Whether the action with the highest probability from the fin...
- **Rubric ID**: 0af6f01e-d9e4-461f-9b2d-63a2565f749d
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Whether the action with the highest probability from the final output matched the action with the highest probability from the output attention head has been computed at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R107: Whether the action with the highest probability from the fin...
- **Rubric ID**: 0628371d-9640-4391-aa6a-79ce7602ebff
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Whether the action with the highest probability from the final output matched the action with the highest probability from the internal policy has been computed at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R108: Whether the action with the highest probability from the out...
- **Rubric ID**: 37e30601-de1c-44e3-a550-8b1358be11e3
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Whether the action with the highest probability from the output attention head matched the action with the highest probability from the internal policy has been computed at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R109: The matching rate for the final output and the output attent...
- **Rubric ID**: ba053a2d-adbc-413f-bd0e-0d86ebe0a741
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The matching rate for the final output and the output attention head has been computed and saved at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R110: The matching rate for the final output and the internal poli...
- **Rubric ID**: f4b0b8c0-5a4d-48b4-bcc9-ad94b6a78559
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The matching rate for the final output and the internal policy has been computed and saved at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R111: The matching rate for the final output and the internal poli...
- **Rubric ID**: 50893fbd-65ab-44b7-83da-7bbc26f84c23
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The matching rate for the final output and the internal policy has been computed and saved at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (b) have been computed and saved.

### R112: The mean matching rate of the final output and the output he...
- **Rubric ID**: 4a93d5d7-0ac9-482b-a1ec-fbd7247c9c52
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean matching rate of the final output and the output head shows that, in the initial timesteps, the final output of the model matches with the result of the output attention head, and the internal policy is barely used.
- **Parent context**: Figure 4 (b) has been reproduced.

### R113: The mean matching rate of the final output and the output he...
- **Rubric ID**: 6c83fb26-cf32-496c-9966-42789b5411e8
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean matching rate of the final output and the output head shows that, after several timesteps, the internal policy learns to imitate the result of the output attention head, which is mostly used as the final output of the model.
- **Parent context**: Figure 4 (b) has been reproduced.

### R114: The mean of the input head's attention scores for all four n...
- **Rubric ID**: e30a1f3d-2374-4be5-b66f-ce3b2b716154
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean of the input head's attention scores for all four non-informative policies drops to around 0.0 within the first 10k timesteps and stays at around 0.0 thereafter.
- **Parent context**: Figure 4 (c) has been reproduced.

### R115: The mean input head's attention scores for the informative p...
- **Rubric ID**: af8a20c1-c252-4a5b-a9b7-28c311a59592
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean input head's attention scores for the informative policy plateaus at roughly 0.16 within the first 200k timesteps and stays at roughly 0.16 thereafter.
- **Parent context**: Figure 4 (c) has been reproduced.

### R116: The mean input head's attention scores for the output attent...
- **Rubric ID**: a96a3a08-2624-4174-bced-17ee5437a0de
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean input head's attention scores for the output attention head sharply increases over the first 200k timesteps then plateaus at roughly 0.8 and stays at roughly 0.8 thereafter.
- **Parent context**: Figure 4 (c) has been reproduced.

### R117: The mean of the output head's attention scores for all four...
- **Rubric ID**: bf94fcd9-5ab6-43cd-8854-6bf9e921a665
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean of the output head's attention scores for all four non-informative policies drops to around 0.0 within the first 10k timesteps and stays at around 0.0 thereafter.
- **Parent context**: Figure 4 (d) has been reproduced.

### R118: The mean output head's attention scores for the informative...
- **Rubric ID**: 1037dc3f-5e4b-41c0-9125-b227f96c5195
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean output head's attention scores for the informative policy sharply increases to 1.0 within the first 10k timesteps, plateaus at 1.0 and stays at 1.0 thereafter.
- **Parent context**: Figure 4 (d) has been reproduced.

### R119: The episodic returns have been computed at regular intervals...
- **Rubric ID**: 231696e0-c9bb-404e-b96f-985dcbd376cb
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The episodic returns have been computed at regular intervals (at least every 10k timesteps) and saved during training of the baseline method for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (e).

### R120: The episodic returns have been computed at regular intervals...
- **Rubric ID**: 17f0e736-c600-4608-ad0c-0513fcc8bd14
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The episodic returns have been computed at regular intervals (at least every 10k timesteps) and saved during training of CompoNet for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (e).

### R121: The saved episodic returns for CompoNet show that the mean e...
- **Rubric ID**: 19014129-a2a7-4e37-b03e-de87cd22da3b
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The saved episodic returns for CompoNet show that the mean episodic returns increases steadily with time, exceeding 400 by 1M timesteps.
- **Parent context**: Figure 4 (e) has been reproduced.

### R122: The saved episodic returns for the baseline method show that...
- **Rubric ID**: b64840ed-52f0-42d3-8fa9-32760c524211
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The saved episodic returns for the baseline method show that the mean episodic returns increases steadily with time, falling short of the mean episodic returns of CompoNet at the 1M timestep mark.
- **Parent context**: Figure 4 (e) has been reproduced.

### R123: Whether the action with the highest probability from the fin...
- **Rubric ID**: c95e4ce4-330f-4c77-9873-7680ac1f7a40
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Whether the action with the highest probability from the final output matched the action with the highest probability from the output attention head has been computed at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R124: Whether the action with the highest probability from the fin...
- **Rubric ID**: ebb72e34-6286-440d-8d90-b3f2c4499818
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Whether the action with the highest probability from the final output matched the action with the highest probability from the internal policy has been computed at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R125: Whether the action with the highest probability from the out...
- **Rubric ID**: f9065e30-749d-4463-9210-931d20dfc599
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: Whether the action with the highest probability from the output attention head matched the action with the highest probability from the internal policy has been computed at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R126: The matching rate for the final output and the output attent...
- **Rubric ID**: 10a23066-6d45-40b7-b14d-208eca4177e1
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The matching rate for the final output and the output attention head has been computed and saved at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R127: The matching rate for the final output and the internal poli...
- **Rubric ID**: 4d4c1c72-7f6e-48e1-baa5-40e24cd61062
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The matching rate for the final output and the internal policy has been computed and saved at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R128: The matching rate for the final output and the internal poli...
- **Rubric ID**: c4f84600-bd06-464d-bf96-c10f8e926039
- **Category**: Code Execution / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The matching rate for the final output and the internal policy has been computed and saved at least every 10k timesteps for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (f).

### R129: The mean matching rate of the final output and the output at...
- **Rubric ID**: 5adbab63-7b1a-4208-92f4-d30ffbcd4b89
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean matching rate of the final output and the output attention head sharply increases to exceed 0.8 (out of a maximum 1.0) within 10k timesteps then plateaus between 0.8 and 1.0.
- **Parent context**: Figure 4 (f) has been reproduced.

### R130: The mean matching rate of the final output and the internal...
- **Rubric ID**: 0bcc219f-78ce-4579-88b9-923345365d52
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean matching rate of the final output and the internal policy is fairly stable at around 0.125 plus or minus 0.125 (out of a maximum 1.0) from the 10k timestep mark to the 300k timestep mark.
- **Parent context**: Figure 4 (f) has been reproduced.

### R131: The mean matching rate of the output head and the internal p...
- **Rubric ID**: ba39e346-3237-4c5b-87bd-769c4cde7395
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean matching rate of the output head and the internal policy is fairly stable at around 0.125 plus or minus 0.125 (out of a maximum 1.0) from the 10k timestep mark to the 300k timestep mark.
- **Parent context**: Figure 4 (f) has been reproduced.

### R132: The mean matching rates show that the final output of the mo...
- **Rubric ID**: 2c237954-542f-4597-b388-042babda1840
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean matching rates show that the final output of the model is completely determined by the internal policy after a few training steps, effectively overwriting the result of the output attention head.
- **Parent context**: Figure 4 (f) has been reproduced.

### R133: The mean of the input head's attention scores for all 5 non-...
- **Rubric ID**: 652b7dd2-1c6c-40ef-ba1c-ae2b54204ad9
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean of the input head's attention scores for all 5 non-informative policies is stable at 0.18 plus or minus 0.125 across all timesteps.
- **Parent context**: Figure 4 (g) has been reproduced.

### R134: The mean input head's attention scores for the output attent...
- **Rubric ID**: 56f1bf39-4aed-499d-a77b-5cdafb5e9c7b
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean input head's attention scores for the output attention head is stable at 0.18 plus or minus 0.125 across all timesteps.
- **Parent context**: Figure 4 (g) has been reproduced.

### R135: No one head has significantly (i.e. > 0.125) higher mean att...
- **Rubric ID**: f846df13-9668-4c25-86ed-1868d672d5e8
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: No one head has significantly (i.e. > 0.125) higher mean attention scores at any timestep.
- **Parent context**: Figure 4 (g) has been reproduced.

### R136: The mean of the output head's attention scores for all 5 non...
- **Rubric ID**: b91fef48-1fe0-481e-b37d-deea55144945
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: The mean of the output head's attention scores for all 5 non-informative policies is stable at 0.2 plus or minus 0.125 across all timesteps.
- **Parent context**: Figure 4 (h) has been reproduced.

### R137: No one head has significantly (i.e. > 0.125) higher mean att...
- **Rubric ID**: 9e0900d2-45e9-47e1-9747-6a219abded90
- **Category**: Result Analysis / Evaluation, Metrics & Benchmarking
- **Weight**: 1
- **Requirement**: No one head has significantly (i.e. > 0.125) higher mean attention scores at any timestep.
- **Parent context**: Figure 4 (h) has been reproduced.

## Environment & Infrastructure Setup

### R138: The `hammer-v2` environment can be built using `gym.make`.
- **Rubric ID**: 6ddede73-5b5b-43aa-b780-71d757075a38
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `hammer-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R139: The `push-wall-v2` environment can be built using `gym.make`...
- **Rubric ID**: 58e40d55-b575-48cd-83c5-5052aaca3a32
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `push-wall-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R140: The `faucet-close-v2` environment can be built using `gym.ma...
- **Rubric ID**: cf4a1d8a-cb3f-4643-a38c-1c37f99999a0
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `faucet-close-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R141: The `push-back-v2` environment can be built using `gym.make`...
- **Rubric ID**: ec9d8fb8-eb16-4226-877b-0d6a0a597338
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `push-back-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R142: The `stick-pull-v2` environment can be built using `gym.make...
- **Rubric ID**: 6c4b5eda-8a21-46f1-a779-46daf3012377
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `stick-pull-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R143: The `handle-press-side-v2` environment can be built using `g...
- **Rubric ID**: e40a1748-6930-40b5-9031-7568c443be95
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `handle-press-side-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R144: The `push-v2` environment can be built using `gym.make`.
- **Rubric ID**: 55a82758-0e16-4f6e-ac19-a0d69103c53e
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `push-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R145: The `shelf-place-v2` environment can be built using `gym.mak...
- **Rubric ID**: 0ada4859-64de-4816-9c5a-c858a74b368f
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `shelf-place-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R146: The `window-close-v2` environment can be built using `gym.ma...
- **Rubric ID**: 86d36b98-9296-4c21-a4ee-397f8d7cce0f
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `window-close-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R147: The `peg-unplug-side-v2` environment can be built using `gym...
- **Rubric ID**: f2ed8992-c22f-46a6-8fa6-faf86b3698c4
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `peg-unplug-side-v2` environment can be built using `gym.make`.
- **Parent context**: All 10 Meta-World environments can be built and use the correct state and action spaces, as describe...

### R148: The Metaworld module (https://github.com/Farama-Foundation/M...
- **Rubric ID**: 915fdd73-cc58-4166-bea6-b729a159383d
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The Metaworld module (https://github.com/Farama-Foundation/Metaworld) is used to define the environments.
- **Parent context**: All Meta-World environments described in Section 5.2 and Appendix D are accessible in code using the...

### R149: The `ALE/SpaceInvaders-v5` environment can be built using `g...
- **Rubric ID**: 65452b97-b69a-4045-ac89-90d44bfa2fc9
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `ALE/SpaceInvaders-v5` environment can be built using `gym.make`.
- **Parent context**: All ALE environments can be built, use the correct observation and action spaces and have all playin...

### R150: The `ALE/Freeway-v5` environment can be built using `gym.mak...
- **Rubric ID**: 5b6e2970-2c60-4199-8fec-63070948738f
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The `ALE/Freeway-v5` environment can be built using `gym.make`.
- **Parent context**: All ALE environments can be built, use the correct observation and action spaces and have all playin...

### R151: The Gymnasium module (https://github.com/Farama-Foundation/G...
- **Rubric ID**: db4ec248-1f0a-4cea-8716-8bd96e347c05
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The Gymnasium module (https://github.com/Farama-Foundation/Gymnasium) is used to define the environments.
- **Parent context**: All ALE environments are accessible in code with the correct observation and action spaces, as descr...

### R152: The PPO implementation from Huang et al. (2022) has been fur...
- **Rubric ID**: 4f2a00e0-423d-4bc2-b674-03e970b79568
- **Category**: Code Development / Environment & Infrastructure Setup
- **Weight**: 1
- **Requirement**: The PPO implementation from Huang et al. (2022) has been further adapted to be compatible with both ALE environments used in the paper: `ALE/SpaceInvaders-v5` and `ALE/Freeway-v5`. TODO: Clarify the specific modifications made with the author.
- **Parent context**: The PPO algorithm has been implemented by adapting the implementation from Huang et al. (2022), as d...

## Experimental Setup

### R153: All 10 tasks in the sequence run for 1M timesteps each for a...
- **Rubric ID**: 09240921-d50d-4490-b1df-84f206fe7eb5
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: All 10 tasks in the sequence run for 1M timesteps each for a total of 20M timesteps, as described in Section 5.2.
- **Parent context**: All Meta-World environments described in Section 5.2 and Appendix D are accessible in code using the...

### R154: All 17 tasks in the sequence run for 1M timesteps each for a...
- **Rubric ID**: e4ba16c0-4d0e-4340-9624-cef226ae7d17
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: All 17 tasks in the sequence run for 1M timesteps each for a total of 17M timesteps, as described in Section 5.2.
- **Parent context**: All ALE environments are accessible in code with the correct observation and action spaces, as descr...

### R155: In the SAC implementation, the optimizer is set to Adam with...
- **Rubric ID**: 520bdc4d-1240-4e8a-8f4d-ce3690d5c246
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the optimizer is set to Adam with $beta_1=0.9$ and $beta_2=0.999$.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R156: In the SAC implementation, the discount rate ($\gamma$) is s...
- **Rubric ID**: 0be98dd1-1403-4cc1-9c4c-000d06187801
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the discount rate ($\gamma$) is set to 0.99.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R157: In the SAC implementation, the maximum and minimum standard...
- **Rubric ID**: e7a7f7c3-6d53-4c68-8a58-bcb39249fdc5
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the maximum and minimum standard deviations for the policy are set to $\exp{2}$ and $\exp{-20}, respectively.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R158: In the SAC implementation, the batch size is set to 128.
- **Rubric ID**: 1144f4b3-0db8-4b1b-8845-b3577b6115fd
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the batch size is set to 128.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R159: In the SAC implementation, the buffer size is set to $10^6$.
- **Rubric ID**: ad325742-df92-4160-8ed3-d4879f63c22f
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the buffer size is set to $10^6$.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R160: In the SAC implementation, the target smoothing coefficient...
- **Rubric ID**: 18b4b952-24c1-4a53-bbc9-024a56978135
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the target smoothing coefficient ($\tau$) is set to 0.005.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R161: In the SAC implementation, the entropy regularization coeffi...
- **Rubric ID**: 1c84cd05-a435-4fe1-ad81-fa4dc3f0bd4c
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the entropy regularization coefficient ($\alpha$) is set to 0.2.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R162: In the SAC implementation, auto-tuning of the entropy regula...
- **Rubric ID**: a735bab4-0624-404a-aca4-b87842654474
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, auto-tuning of the entropy regularization coefficient ($\alpha$) is enabled.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R163: In the SAC implementation, the policy update frequency is se...
- **Rubric ID**: b49c1a3a-e280-41f7-bb65-a1568fa173a3
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the policy update frequency is set to 2.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R164: In the SAC implementation, the target network update frequen...
- **Rubric ID**: 72f5c11c-ec26-4fdc-be40-ea41351c560f
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the target network update frequency is set to 1.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R165: In the SAC implementation, the noise clip parameter is set t...
- **Rubric ID**: 330eea86-52b8-4511-9123-b66a3857db0e
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the noise clip parameter is set to 0.5.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R166: In the SAC implementation, the number of random actions is s...
- **Rubric ID**: eac55aec-805d-4b31-8ae3-7b9158a40d6c
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the number of random actions is set to $10^4$.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R167: In the SAC implementation, the timestep to start learning is...
- **Rubric ID**: 1fb99226-8c58-4097-9a98-85dc6f64aaf1
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the timestep to start learning is set to $5 \times 10^3$.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R168: In the SAC implementation, the actors learning rate is set t...
- **Rubric ID**: 652d8621-d466-400c-a2b8-100c02f042d6
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the actors learning rate is set to $10^{-3}$.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R169: In the SAC implementation, the Q-networks' learning rate is...
- **Rubric ID**: 85760c29-b7fc-4d86-9555-e40cf1607d6f
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the SAC implementation, the Q-networks' learning rate is set to $10^{-3}$.
- **Parent context**: The hyperparameters for SAC have been set to those stated in Table E.1.

### R170: In the PPO implementation, the optimizer has been set to Ada...
- **Rubric ID**: 04a582b9-5473-4d89-a2ff-c7e924acfd55
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the optimizer has been set to AdamW.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R171: In the PPO implementation, the $beta_1$ and $beta_2$ paramet...
- **Rubric ID**: b5c20ebb-50da-4022-aed6-4cf969d67ad9
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the $beta_1$ and $beta_2$ parameters of AdamW are set to 0.9 and 0.999 respectively.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R172: In the PPO implementation, the maximum gradient norm is set...
- **Rubric ID**: 24277bc2-8e61-457e-af20-c13d18cab57a
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the maximum gradient norm is set to 0.5.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R173: In the PPO implementation, the discount rate ($\gamma$) has...
- **Rubric ID**: b181f0b9-54d1-4030-8ed3-b52ef0f54b1a
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the discount rate ($\gamma$) has been set to 0.99.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R174: In the PPO implementation, the hidden dimension ($d_{model})...
- **Rubric ID**: 61cf8fe3-10e8-414e-9b01-5f12b6e89bf1
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the hidden dimension ($d_{model}) is set to 512.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R175: In the PPO implementation, the learning rate is set to $2.5...
- **Rubric ID**: 9d838de5-2888-4431-8b57-5bb00dffe3a0
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the learning rate is set to $2.5 \cdot 10^{-4}$.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R176: In the PPO implementation, the PPO value function coefficien...
- **Rubric ID**: d8b7555f-3b55-4214-908b-e588e394c1cc
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the PPO value function coefficient is set to 0.5.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R177: In the PPO implementation, the GAE ($\lambda$) is set to 0.9...
- **Rubric ID**: 0193f558-fbfb-407d-acc5-5f122c306beb
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the GAE ($\lambda$) is set to 0.95.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R178: In the PPO implementation, the number of parallel environmen...
- **Rubric ID**: 940b2a8a-03be-4bfd-8cbe-abb88206f55c
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the number of parallel environments is set to 8.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R179: In the PPO implementation, the batch size is set to 1024.
- **Rubric ID**: 27f990db-3c95-4443-a9e0-74cae3cc7798
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the batch size is set to 1024.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R180: In the PPO implementation, the number of update epochs is se...
- **Rubric ID**: eda94d3e-be8e-476b-a1b1-fc4034bd69f7
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the number of update epochs is set to 4.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R181: In the PPO implementation, the PPO clipping coefficient is s...
- **Rubric ID**: 97903a43-5146-4ed5-8fdf-73aa04792903
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the PPO clipping coefficient is set to 0.2.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R182: In the PPO implementation, the PPO entropy coefficient is se...
- **Rubric ID**: b159646a-15bb-41a2-b041-455aadb3819e
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the PPO entropy coefficient is set to 0.01.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R183: In the PPO implementation, learning rate annealing is enable...
- **Rubric ID**: c68146ee-62e0-436e-981c-afe6a4b5f65f
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, learning rate annealing is enabled.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R184: In the PPO implementation, the number of steps per rollout i...
- **Rubric ID**: f7bb6f64-757c-441d-9ae3-1df0adf3135e
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: In the PPO implementation, the number of steps per rollout is set to 128.
- **Parent context**: The hyperparameters for PPO have been set to those stated in Table E.2.

### R185: The baseline method has been implemented such that a randoml...
- **Rubric ID**: 96dbde05-2acb-4fde-a279-74e326a2254d
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: The baseline method has been implemented such that a randomly initialized neural network is trained from scratch for each task.
- **Parent context**: The baseline method has been implemented for the Meta-World and ALE environments, as described in Se...

### R186: The logic to save model parameters after training on each ta...
- **Rubric ID**: 1b7ece23-8d81-446a-8af4-d468526c81e6
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: The logic to save model parameters after training on each task has been implemented.
- **Parent context**: FT-N has been implemented for the Meta-World and ALE environments, as described in Section 5.2 and A...

### R187: The number of iterations to retrain the model is set to 200k...
- **Rubric ID**: c5ff509c-087d-4cae-a1ff-63dad10de6b1
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: The number of iterations to retrain the model is set to 200k steps (i.e., 20% of the 1M timestep budget of each task).
- **Parent context**: PackNet has been implemented, as described in Section 5.2 and Appendix E.2. TODO: Remove and replace...

### R188: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: dabaeb40-3ffd-48d6-a28d-b04ccbf351b5
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The baseline method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R189: A total of 1M timesteps have been used per task.
- **Rubric ID**: 066903cf-56bc-4aa1-8118-a12005ed0f9e
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The baseline method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R190: The baseline method has been trained on the Meta-World seque...
- **Rubric ID**: 8036351f-b10a-4939-9b7d-1c7c65148801
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The baseline method has been trained on the Meta-World sequence of tasks for each seed.
- **Parent context**: The baseline method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R191: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: eb472db9-5135-44d7-af2c-c21a4cbbfe21
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The FT-1 method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R192: A total of 1M timesteps have been used per task.
- **Rubric ID**: 9451e9fd-24cd-4424-9219-2d856c05643b
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The FT-1 method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R193: The FT-1 method has been trained on the Meta-World sequence...
- **Rubric ID**: 70670ea3-ae33-447d-a616-606c714f1015
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The FT-1 method has been trained on the Meta-World sequence of tasks for each seed.
- **Parent context**: The FT-1 method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R194: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 700e4ef4-e38b-4803-907d-f4a9c7cf2b8b
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The FT-N method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R195: A total of 1M timesteps have been used per task.
- **Rubric ID**: 4711b5df-5dc4-4ba3-a10d-a1ca33a73e1a
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The FT-N method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R196: The FT-N method has been trained on the Meta-World sequence...
- **Rubric ID**: dd84f4ed-e5e6-45c9-9028-915b465f24e0
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The FT-N method has been trained on the Meta-World sequence of tasks for each seed.
- **Parent context**: The FT-N method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R197: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 30625bbc-8a06-4668-9436-322778e96ea4
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The ProgressiveNet method has been trained on the Meta-World sequence of tasks, and the success rate...

### R198: A total of 1M timesteps have been used per task.
- **Rubric ID**: b43f9fb7-149a-4414-ab3d-103474f78214
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The ProgressiveNet method has been trained on the Meta-World sequence of tasks, and the success rate...

### R199: The ProgressiveNet method has been trained on the Meta-World...
- **Rubric ID**: 0405123a-7154-4003-b716-050b55a344b6
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The ProgressiveNet method has been trained on the Meta-World sequence of tasks for each seed.
- **Parent context**: The ProgressiveNet method has been trained on the Meta-World sequence of tasks, and the success rate...

### R200: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 9d20e815-7686-4924-841b-40463fa1b1c2
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The PackNet method has been trained on the Meta-World sequence of tasks, and the success rates have ...

### R201: A total of 1M timesteps have been used per task.
- **Rubric ID**: 61ca21ef-87c0-4c63-bd8f-d77526afea15
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The PackNet method has been trained on the Meta-World sequence of tasks, and the success rates have ...

### R202: The PackNet method has been trained on the Meta-World sequen...
- **Rubric ID**: f6993b3e-2a5e-4375-b92a-60daa1e6e169
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The PackNet method has been trained on the Meta-World sequence of tasks for each seed.
- **Parent context**: The PackNet method has been trained on the Meta-World sequence of tasks, and the success rates have ...

### R203: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 7718e9e1-71a0-4d1e-9d90-76d4b0212af1
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The CompoNet method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R204: A total of 1M timesteps have been used per task.
- **Rubric ID**: a648826d-bd2f-4192-adfe-4640bd9748ab
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The CompoNet method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R205: The CompoNet method has been trained on the Meta-World seque...
- **Rubric ID**: d5f9b44a-9d65-4974-bf55-91d86c690b38
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The CompoNet method has been trained on the Meta-World sequence of tasks for each seed.
- **Parent context**: The CompoNet method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R206: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 4cae0a3a-f411-4486-8abb-d4ba75b08db3
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The baseline method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R207: A total of 1M timesteps have been used per task.
- **Rubric ID**: 1504ba8d-3cd7-4384-b633-5363ecf1e72d
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The baseline method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R208: The baseline method has been trained on the SpaceInvaders se...
- **Rubric ID**: a1e3bc32-dcd3-4b3f-994a-5674da57adc5
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The baseline method has been trained on the SpaceInvaders sequence of tasks for each seed.
- **Parent context**: The baseline method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R209: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 2bd5f28c-0d6b-46f1-a7f8-18aabf02d6bc
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The FT-1 method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R210: A total of 1M timesteps have been used per task.
- **Rubric ID**: ce642948-5102-4718-805e-f966f19a5d5b
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The FT-1 method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R211: The FT-1 method has been trained on the SpaceInvaders sequen...
- **Rubric ID**: b86c0b3a-0b73-4260-bf92-85c8db218b2a
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The FT-1 method has been trained on the SpaceInvaders sequence of tasks for each seed.
- **Parent context**: The FT-1 method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R212: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: fe1faf1b-aa2f-413d-9a1d-157c73d2912c
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The FT-N method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R213: A total of 1M timesteps have been used per task.
- **Rubric ID**: 492ff920-96ed-42e9-a64c-f44b4a4377c6
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The FT-N method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R214: The FT-N method has been trained on the SpaceInvaders sequen...
- **Rubric ID**: 5f9d4a61-2534-4f8e-a30b-a5b0402a9077
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The FT-N method has been trained on the SpaceInvaders sequence of tasks for each seed.
- **Parent context**: The FT-N method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R215: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: b773c1d6-8a03-45a6-86d4-258f83e283f7
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The ProgressiveNet method has been trained on the SpaceInvaders sequence of tasks, and the success r...

### R216: A total of 1M timesteps have been used per task.
- **Rubric ID**: 5907a98f-b3db-4d81-8f09-ac1616e3524a
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The ProgressiveNet method has been trained on the SpaceInvaders sequence of tasks, and the success r...

### R217: The ProgressiveNet method has been trained on the SpaceInvad...
- **Rubric ID**: 22259e64-ae90-482c-948f-b4abef319a5a
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The ProgressiveNet method has been trained on the SpaceInvaders sequence of tasks for each seed.
- **Parent context**: The ProgressiveNet method has been trained on the SpaceInvaders sequence of tasks, and the success r...

### R218: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: b5fee7bc-a717-447e-95f7-8e2f26d18f39
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The PackNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates ha...

### R219: A total of 1M timesteps have been used per task.
- **Rubric ID**: d759301e-34d3-4b0a-b848-75868481e26c
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The PackNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates ha...

### R220: The PackNet method has been trained on the SpaceInvaders seq...
- **Rubric ID**: 92a9974e-e8f5-46ef-8e7e-e88db75ca5ee
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The PackNet method has been trained on the SpaceInvaders sequence of tasks for each seed.
- **Parent context**: The PackNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates ha...

### R221: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 68554bdf-415c-4d26-9b61-67aeaebc83f6
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The CompoNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R222: A total of 1M timesteps have been used per task.
- **Rubric ID**: d8d72917-527b-4f80-bbb2-b8453364a47b
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The CompoNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R223: The CompoNet method has been trained on the SpaceInvaders se...
- **Rubric ID**: 17775db7-f3bb-42ac-8c3f-072bc3cfd295
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The CompoNet method has been trained on the SpaceInvaders sequence of tasks for each seed.
- **Parent context**: The CompoNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R224: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 7a67fb64-e1ec-4b8a-b851-627d13c09be6
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The baseline method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R225: A total of 1M timesteps have been used per task.
- **Rubric ID**: 09d2eeb3-61bc-4199-87f4-ba1c057fef3e
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The baseline method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R226: The baseline method has been trained on the Freeway sequence...
- **Rubric ID**: 3a8dbbae-fd7b-4e72-abd7-45bb43fe6a4b
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The baseline method has been trained on the Freeway sequence of tasks for each seed.
- **Parent context**: The baseline method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R227: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 01488f67-5d87-4099-926b-facc9d36cc36
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The FT-1 method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R228: A total of 1M timesteps have been used per task.
- **Rubric ID**: cdbd9710-288a-4a37-b729-c5c7587be0cd
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The FT-1 method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R229: The FT-1 method has been trained on the Freeway sequence of...
- **Rubric ID**: 1603bab6-b73f-40f7-8e86-30d3bce8e320
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The FT-1 method has been trained on the Freeway sequence of tasks for each seed.
- **Parent context**: The FT-1 method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R230: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: d61db658-ea2a-4ac1-acb4-fb3e20209685
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The FT-N method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R231: A total of 1M timesteps have been used per task.
- **Rubric ID**: 7555d599-94f4-4b5d-ad00-0a903874cda5
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The FT-N method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R232: The FT-N method has been trained on the Freeway sequence of...
- **Rubric ID**: 8e5f55ac-9f45-4f42-82bb-6981ce0b5439
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The FT-N method has been trained on the Freeway sequence of tasks for each seed.
- **Parent context**: The FT-N method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R233: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 61cb9780-690b-4089-ace0-d0ec3d3f97a8
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The ProgressiveNet method has been trained on the Freeway sequence of tasks, and the success rates h...

### R234: A total of 1M timesteps have been used per task.
- **Rubric ID**: c5442170-3c69-4291-86c3-0052de16709c
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The ProgressiveNet method has been trained on the Freeway sequence of tasks, and the success rates h...

### R235: The ProgressiveNet method has been trained on the Freeway se...
- **Rubric ID**: e91b4c0b-149c-463f-8ace-071c11f62e48
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The ProgressiveNet method has been trained on the Freeway sequence of tasks for each seed.
- **Parent context**: The ProgressiveNet method has been trained on the Freeway sequence of tasks, and the success rates h...

### R236: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 7ea912da-7f8b-43a5-8324-03e9e202bcda
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The PackNet method has been trained on the Freeway sequence of tasks, and the success rates have bee...

### R237: A total of 1M timesteps have been used per task.
- **Rubric ID**: c1765eed-a025-4678-86b3-8699ef704048
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The PackNet method has been trained on the Freeway sequence of tasks, and the success rates have bee...

### R238: The PackNet method has been trained on the Freeway sequence...
- **Rubric ID**: b04f9b98-31cc-4cf0-9d68-bbe40d7001e8
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The PackNet method has been trained on the Freeway sequence of tasks for each seed.
- **Parent context**: The PackNet method has been trained on the Freeway sequence of tasks, and the success rates have bee...

### R239: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: dd047fca-a742-4f6f-90a8-2fe069df3ead
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: The CompoNet method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R240: A total of 1M timesteps have been used per task.
- **Rubric ID**: 3e25f5ab-bec4-4aba-a4e7-eacd5f7b22cb
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 1M timesteps have been used per task.
- **Parent context**: The CompoNet method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R241: The CompoNet method has been trained on the Freeway sequence...
- **Rubric ID**: f04bc474-d56e-40e3-bfe2-f479807bee0d
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The CompoNet method has been trained on the Freeway sequence of tasks for each seed.
- **Parent context**: The CompoNet method has been trained on the Freeway sequence of tasks, and the success rates have be...

### R242: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: 4dca3b7d-1188-4337-961e-dd6a0d5ac1f7
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: Figures 4 (a) - (d) have been reproduced. CompoNet is trained on the 5th task (i.e., the 4th playing...

### R243: The newly added CompoNet module has been trained for 1M time...
- **Rubric ID**: 75042767-b7b9-4225-8297-f082f7390dc3
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The newly added CompoNet module has been trained for 1M timesteps on the 5th task of SpaceInvaders using the SAC algorithm, while keeping the parameters of all previous modules frozen.
- **Parent context**: CompoNet was trained on the 5th task (i.e., the 4th playing mode) of SpaceInvaders for 1M timesteps ...

### R244: The baseline method was trained on the 5th task (i.e., the 4...
- **Rubric ID**: 4f725a8e-0163-4a5c-b99a-52de8a26aefd
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The baseline method was trained on the 5th task (i.e., the 4th playing mode) of SpaceInvaders for 1M timesteps per seed using the SAC algorithm and the hyperparameters in Table E.1 (TODO: Confirm with the author, remove once confirmed).
- **Parent context**: Figures 4 (a) - (d) have been reproduced. CompoNet is trained on the 5th task (i.e., the 4th playing...

### R245: A total of 10 random seeds have been set before each trainin...
- **Rubric ID**: a52a28e5-be1f-40f3-9784-734978d353d8
- **Category**: Code Development / Experimental Setup
- **Weight**: 1
- **Requirement**: A total of 10 random seeds have been set before each training run.
- **Parent context**: Figures 4 (e) - (h) have been reproduced. CompoNet is trained on the 6th task (i.e., the 5th playing...

### R246: The newly added CompoNet module has been trained for 1M time...
- **Rubric ID**: 57fe4f9d-e36d-4b55-80c3-23e3fa442292
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The newly added CompoNet module has been trained for 1M timesteps on the 6th task (i.e., the 5th playing mode) of SpaceInvaders using the SAC algorithm.
- **Parent context**: CompoNet was trained on the 6th task (i.e., the 5th playing mode) of SpaceInvaders for 1M timesteps ...

### R247: The baseline method was trained on the 6th task (i.e., the 5...
- **Rubric ID**: 315a31da-6a39-4be1-881b-4e5a29d46413
- **Category**: Code Execution / Experimental Setup
- **Weight**: 1
- **Requirement**: The baseline method was trained on the 6th task (i.e., the 5th playing mode) of SpaceInvaders for 1M timesteps per seed using the SAC algorithm and the hyperparameters in Table E.1 (TODO: Confirm with the author, remove once confirmed).
- **Parent context**: Figures 4 (e) - (h) have been reproduced. CompoNet is trained on the 6th task (i.e., the 5th playing...

## Logging, Analysis & Presentation

### R248: The recorded results show that the inference time (in second...
- **Rubric ID**: 56f8b909-59d1-4bec-93c2-f276d499694e
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The recorded results show that the inference time (in seconds) of ProgressiveNet grows quadratically.
- **Parent context**: The inference time (left) sub-figure has been reproduced.

### R249: The recorded results show that the inference time (in second...
- **Rubric ID**: 8d795b81-0cc9-41de-8d76-314ed523ff4e
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The recorded results show that the inference time (in seconds) of CompoNet grows slower than ProgressiveNet, with the gap widening as the number of tasks increases.
- **Parent context**: The inference time (left) sub-figure has been reproduced.

### R250: The recorded results show that the total parameter count of...
- **Rubric ID**: f0dafbe8-c135-44e6-a758-95a62a85cf02
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The recorded results show that the total parameter count of CompoNet grows (roughly) linearly.
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R251: The recorded results show that the trainable parameter count...
- **Rubric ID**: 23a2aa0a-1970-4dc7-95ca-ebe70fb903c4
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The recorded results show that the trainable parameter count of CompoNet grows linearly.
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R252: The recorded results show that the total parameter count of...
- **Rubric ID**: 318d3eee-9f08-4b3b-ae34-e0bbc5d5b139
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The recorded results show that the total parameter count of ProgressiveNet grows quadratically.
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R253: The recorded results show that the trainable parameter count...
- **Rubric ID**: 1df26f0a-7d45-4673-b0f4-caf4698bf86b
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The recorded results show that the trainable parameter count of ProgressiveNet grows quadratically.
- **Parent context**: The growth of the number of parameters (right) sub-figure has been reproduced.

### R254: The success rates for each task and seed has been recorded.
- **Rubric ID**: 40f3d0f3-69b7-4c5b-a800-e412b3f16c93
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The baseline method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R255: The success rates for each task and seed has been recorded.
- **Rubric ID**: 68b39bd1-8e5c-4bc8-94f0-c260fd761df7
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The FT-1 method has been trained on the Meta-World sequence of tasks, and the success rates have bee...

### R256: The success rates for each task and seed has been recorded.
- **Rubric ID**: ae3146a2-d1b5-40e4-bc22-d299fc05d8ec
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The ProgressiveNet method has been trained on the Meta-World sequence of tasks, and the success rate...

### R257: The success rates for each task and seed has been recorded.
- **Rubric ID**: 3bd4f029-ef91-490e-9f02-12e4ca3d378c
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The PackNet method has been trained on the Meta-World sequence of tasks, and the success rates have ...

### R258: The success rates for each task and seed has been recorded.
- **Rubric ID**: 90c21094-09bd-462e-92fa-0ef7907ff1ed
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The CompoNet method has been trained on the Meta-World sequence of tasks, and the success rates have...

### R259: The success rates for each task and seed has been recorded.
- **Rubric ID**: a965f76c-59eb-4b16-86a7-aeb8cddce8a6
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The FT-1 method has been trained on the SpaceInvaders sequence of tasks, and the success rates have ...

### R260: The success rates for each task and seed has been recorded.
- **Rubric ID**: 095b2f89-c096-49d4-b94a-9c233860878c
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The ProgressiveNet method has been trained on the SpaceInvaders sequence of tasks, and the success r...

### R261: The success rates for each task and seed has been recorded.
- **Rubric ID**: f58282f6-4838-4847-8c56-b20fef5e97c0
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The PackNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates ha...

### R262: The success rates for each task and seed has been recorded.
- **Rubric ID**: 8353a180-2494-4418-89f9-5a623f5717c0
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The CompoNet method has been trained on the SpaceInvaders sequence of tasks, and the success rates h...

### R263: The success rates for each task and seed has been recorded.
- **Rubric ID**: e4ebeeb5-501f-438d-b913-f67ad2e06ab5
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The FT-1 method has been trained on the Freeway sequence of tasks, and the success rates have been s...

### R264: The success rates for each task and seed has been recorded.
- **Rubric ID**: e18fac10-67ed-4ac8-9603-440b2c034ba6
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The success rates for each task and seed has been recorded.
- **Parent context**: The ProgressiveNet method has been trained on the Freeway sequence of tasks, and the success rates h...

### R265: The episodic returns have been computed at regular intervals...
- **Rubric ID**: ca4ae1dd-5250-4b07-a40d-4995876fab3a
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The episodic returns have been computed at regular intervals (at least every 10k timesteps) and saved during training of the baseline method for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (a) have been computed and saved.

### R266: The saved episodic returns for CompoNet show that the mean e...
- **Rubric ID**: c0916ef4-ee7a-43a0-9923-8004a18cd67c
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The saved episodic returns for CompoNet show that the mean episodic returns sharply increases within the first 200k timesteps, then plateaus at around 600.
- **Parent context**: Figure 4 (a) has been reproduced.

### R267: The saved episodic returns for the baseline method show that...
- **Rubric ID**: d7e80724-6356-4172-b3e1-bfd2accdfc0c
- **Category**: Result Analysis / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The saved episodic returns for the baseline method show that the mean episodic returns steadily increases over time, eventually reaching an episodic return of around 400 by the 1M timestep mark.
- **Parent context**: Figure 4 (a) has been reproduced.

### R268: The input attention head's attention distribution (i.e., the...
- **Rubric ID**: 44e32f85-016d-404e-8605-6700774b3e9c
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The input attention head's attention distribution (i.e., the output of the softmax) over the rows of $P$ has been computed at regular intervals (at least every 10k timesteps) during the training loop of CompoNet for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (c) have been computed and saved,

### R269: The input attention head's attention distributions over the...
- **Rubric ID**: c20e0d24-aabb-4450-b3be-d8c6d743b505
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The input attention head's attention distributions over the rows of $P$ has been saved at regular intervals (at least every 10k timesteps) for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (c) have been computed and saved,

### R270: The output attention head's attention distribution (i.e., th...
- **Rubric ID**: b92a3750-8f72-4460-b46c-f9cf8ea6014b
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The output attention head's attention distribution (i.e., the output of the softmax) over all previous policies has been computed at regular intervals (at least every 10k timesteps) during CompoNet training for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (d) have been computed and saved.

### R271: The output attention head's attention distributions have bee...
- **Rubric ID**: d6ae56b2-c763-4ac3-80be-d0758d5869c9
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The output attention head's attention distributions have been saved at regular intervals (at least every 10k timesteps) for each seed.
- **Parent context**: The necessary metrics to reproduce Figure 4 (d) have been computed and saved.

### R272: The input attention head's attention distribution (i.e., the...
- **Rubric ID**: f869125d-bead-4e59-b8b5-978c7cb04f5b
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The input attention head's attention distribution (i.e., the output of the softmax) over the rows of $P$ has been computed at regular intervals (at least every 10k timesteps) during the training loop of CompoNet for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (g).

### R273: The input attention head's attention distributions over the...
- **Rubric ID**: 1fdca623-492f-432e-9f14-06c2cec7a476
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The input attention head's attention distributions over the rows of $P$ has been saved at regular intervals (at least every 10k timesteps) for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (g).

### R274: The output attention head's attention distribution (i.e., th...
- **Rubric ID**: c42318bd-1e41-4eb3-a5f7-185275ea482c
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The output attention head's attention distribution (i.e., the output of the softmax) over all previous policies has been computed at regular intervals (at least every 10k timesteps) during CompoNet training for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (h).

### R275: The output attention head's attention distributions have bee...
- **Rubric ID**: 2a1284a1-91dc-45e3-a19f-e500c945c567
- **Category**: Code Execution / Logging, Analysis & Presentation
- **Weight**: 1
- **Requirement**: The output attention head's attention distributions have been saved at regular intervals (at least every 10k timesteps) for each seed.
- **Parent context**: The necessary metrics have computed and saved to reproduce Figure 4 (h).
